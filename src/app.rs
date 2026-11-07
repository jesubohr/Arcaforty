//! Application state and behavior. Rendering is in `views.rs`.
//!
//! Slow work (key derivation, file access) runs on a background thread through `run`.
//! While it runs, `busy` is set and event handlers ignore input that would change data.

use crate::clipboard;
use crate::field::{Changed, Submit, TextField};
use crate::theme::Palette;
use arcaforty_core::import::{MAX_IMPORT_BYTES, Parsed, parse_csv, parse_env};
use arcaforty_core::{
    Entry, Error, InstanceLock, Kind, MAX_FILE_BYTES, Settings, Store, Vault, check_new_password,
    limits, now, read_bounded,
};
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, PathPromptOptions, SharedString,
    UniformListScrollHandle, Window, actions,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

actions!(
    app,
    [Back, LockNow, NewSecret, FocusSearch, NextField, PrevField]
);

#[derive(Clone, PartialEq)]
pub enum Screen {
    Fatal(SharedString),
    Setup,
    Unlock,
    List,
    Detail(String),
    Edit,
    Settings,
    Import,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Section {
    General,
    Security,
    Data,
}

pub struct Notice {
    pub text: SharedString,
    pub error: bool,
    until: Instant,
}

/// What the list shows for one secret. Never holds the secret value.
pub struct Row {
    pub id: String,
    pub platform: SharedString,
    pub subtitle: SharedString,
    pub kind: Kind,
    pub updated: i64,
}

pub struct ImportState {
    pub source: SharedString,
    pub entries: Vec<Entry>,
    pub chosen: Vec<bool>,
    pub warnings: Vec<String>,
}

struct Clip {
    text: Zeroizing<String>,
    deadline: Instant,
}

pub struct Root {
    pub store: Store,
    _lock: Option<InstanceLock>,
    pub settings: Settings,
    settings_saving: bool,
    settings_dirty: bool,
    pub vault: Option<Vault>,
    pub screen: Screen,
    pub busy: Option<&'static str>,
    generation: u64,
    /// A native file dialog is open. It outlives a lock, so only its own result clears this.
    dialog_open: bool,
    pub notice: Option<Notice>,
    pub form_error: Option<SharedString>,
    last_activity: Instant,
    pub focus: FocusHandle,

    pub password: Entity<TextField>,
    pub confirm: Entity<TextField>,
    pub search: Entity<TextField>,
    pub platform: Entity<TextField>,
    pub value: Entity<TextField>,
    pub notes: Entity<TextField>,
    pub current_pw: Entity<TextField>,
    pub new_pw: Entity<TextField>,
    pub new_pw2: Entity<TextField>,
    pub backup_pw: Entity<TextField>,
    pub backup_pw2: Entity<TextField>,
    pub restore_pw: Entity<TextField>,

    pub filter: Option<Kind>,
    pub rows: Vec<Row>,
    pub total: usize,
    pub list_scroll: UniformListScrollHandle,
    pub import_scroll: UniformListScrollHandle,
    pub reveal_until: Option<Instant>,
    pub confirm_delete: bool,
    pub editing: Option<String>,
    pub edit_kind: Kind,
    pub section: Section,
    pub import: Option<ImportState>,
    clip: Option<Clip>,
}

impl Focusable for Root {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn first_line(notes: &str) -> SharedString {
    let line = notes
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    line.chars().take(120).collect::<String>().into()
}

impl Root {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = Store::default_location();
        let (lock, screen) = match store.lock_instance() {
            Ok(lock) => (
                Some(lock),
                if store.exists() {
                    Screen::Unlock
                } else {
                    Screen::Setup
                },
            ),
            Err(e) => (None, Screen::Fatal(e.to_string().into())),
        };
        let settings = store.load_settings();

        let field = |cx: &mut Context<Self>, hint: &str, max: usize| {
            cx.new(|cx| TextField::new(cx, hint, max))
        };
        let secret = |cx: &mut Context<Self>, hint: &str| {
            cx.new(|cx| TextField::new(cx, hint, 1024).masked())
        };
        let password = secret(cx, "");
        let confirm = secret(cx, "");
        let search = field(cx, "Search secrets", 200);
        let platform = field(cx, "", limits::PLATFORM);
        let value = cx.new(|cx| {
            TextField::new(cx, "", limits::VALUE)
                .multiline(6)
                .mono()
                .masked()
        });
        let notes = cx.new(|cx| {
            TextField::new(
                cx,
                "Where it is used, which account it belongs to, when it expires",
                limits::NOTES,
            )
            .multiline(3)
        });
        let current_pw = secret(cx, "");
        let new_pw = secret(cx, "");
        let new_pw2 = secret(cx, "");
        let backup_pw = secret(cx, "");
        let backup_pw2 = secret(cx, "");
        let restore_pw = secret(cx, "");

        let focus = cx.focus_handle();
        let mut root = Root {
            store,
            _lock: lock,
            settings,
            settings_saving: false,
            settings_dirty: false,
            vault: None,
            screen,
            busy: None,
            generation: 0,
            dialog_open: false,
            notice: None,
            form_error: None,
            last_activity: Instant::now(),
            focus,
            password,
            confirm,
            search,
            platform,
            value,
            notes,
            current_pw,
            new_pw,
            new_pw2,
            backup_pw,
            backup_pw2,
            restore_pw,
            filter: None,
            rows: Vec::new(),
            total: 0,
            list_scroll: UniformListScrollHandle::new(),
            import_scroll: UniformListScrollHandle::new(),
            reveal_until: None,
            confirm_delete: false,
            editing: None,
            edit_kind: Kind::ApiKey,
            section: Section::General,
            import: None,
            clip: None,
        };
        root.apply_theme(window, cx);
        root.wire(window, cx);
        root.focus_first(window, cx);
        root
    }

    /// Event subscriptions and the one second timer.
    fn wire(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.observe_window_appearance(window, |this, window, cx| this.apply_theme(window, cx))
            .detach();
        cx.on_app_quit(|this, cx| {
            if let Some(clip) = this.clip.take() {
                clipboard::clear_if_holds(cx, &clip.text);
            }
            async {}
        })
        .detach();

        cx.subscribe_in(&self.search, window, |this, _, _: &Changed, _, cx| {
            this.refresh_rows(cx);
            this.list_scroll
                .scroll_to_item(0, gpui::ScrollStrategy::Top);
            cx.notify();
        })
        .detach();

        let (password, confirm) = (self.password.clone(), self.confirm.clone());
        cx.subscribe_in(&password, window, |this, _, _: &Submit, window, cx| {
            if this.screen == Screen::Setup && this.confirm.read(cx).text().is_empty() {
                window.focus(&this.confirm.focus_handle(cx));
            } else {
                this.submit_auth(window, cx);
            }
        })
        .detach();
        cx.subscribe_in(&confirm, window, |this, _, _: &Submit, window, cx| {
            this.submit_auth(window, cx)
        })
        .detach();
        cx.subscribe_in(&self.platform, window, |_, _, _: &Submit, window, _| {
            window.focus_next()
        })
        .detach();
        cx.subscribe_in(&self.current_pw, window, |_, _, _: &Submit, window, _| {
            window.focus_next()
        })
        .detach();
        cx.subscribe_in(&self.new_pw, window, |_, _, _: &Submit, window, _| {
            window.focus_next()
        })
        .detach();
        cx.subscribe_in(&self.new_pw2, window, |this, _, _: &Submit, window, cx| {
            this.change_password(window, cx)
        })
        .detach();
        cx.subscribe_in(&self.backup_pw, window, |_, _, _: &Submit, window, _| {
            window.focus_next()
        })
        .detach();
        cx.subscribe_in(
            &self.backup_pw2,
            window,
            |this, _, _: &Submit, window, cx| this.export_backup(window, cx),
        )
        .detach();
        cx.subscribe_in(
            &self.restore_pw,
            window,
            |this, _, _: &Submit, window, cx| this.restore_backup(window, cx),
        )
        .detach();

        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update_in(cx, |this, window, cx| this.tick(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    // ----- small helpers -----

    pub fn palette(&self, cx: &App) -> Palette {
        *cx.global::<Palette>()
    }

    pub fn is_locked_screen(&self) -> bool {
        matches!(
            self.screen,
            Screen::Fatal(_) | Screen::Setup | Screen::Unlock
        )
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn apply_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.set_global(Palette::resolve(
            self.settings.appearance,
            window.appearance(),
        ));
        cx.notify();
    }

    fn focus_first(&self, window: &mut Window, cx: &App) {
        let target = match self.screen {
            Screen::Setup | Screen::Unlock => Some(&self.password),
            Screen::List => Some(&self.search),
            _ => None,
        };
        match target {
            Some(field) => window.focus(&field.focus_handle(cx)),
            None => window.focus(&self.focus),
        }
    }

    pub fn show(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = screen;
        self.form_error = None;
        self.focus_first(window, cx);
        cx.notify();
    }

    pub fn say(&mut self, text: impl Into<SharedString>, error: bool, cx: &mut Context<Self>) {
        let secs = if error { 10 } else { 6 };
        self.notice = Some(Notice {
            text: text.into(),
            error,
            until: Instant::now() + Duration::from_secs(secs),
        });
        cx.notify();
    }

    fn all_fields(&self) -> [&Entity<TextField>; 12] {
        [
            &self.password,
            &self.confirm,
            &self.search,
            &self.platform,
            &self.value,
            &self.notes,
            &self.current_pw,
            &self.new_pw,
            &self.new_pw2,
            &self.backup_pw,
            &self.backup_pw2,
            &self.restore_pw,
        ]
    }

    pub fn clear_fields(&self, fields: &[&Entity<TextField>], cx: &mut Context<Self>) {
        for f in fields {
            f.update(cx, |f, cx| f.clear(cx));
        }
    }

    fn text_of(&self, field: &Entity<TextField>, cx: &App) -> Zeroizing<String> {
        Zeroizing::new(field.read(cx).text().to_string())
    }

    /// Run `job` on a background thread. `done` runs on the UI thread unless the vault was
    /// locked in the meantime.
    fn run<T: Send + 'static>(
        &mut self,
        window: &Window,
        label: &'static str,
        cx: &mut Context<Self>,
        job: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.busy = Some(label);
        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            let out = cx.background_executor().spawn(async move { job() }).await;
            this.update_in(cx, |this, window, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = None;
                done(this, out, window, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    // ----- timer -----

    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        let idle = Duration::from_secs(u64::from(self.settings.idle_lock_minutes) * 60);
        if !self.is_locked_screen() && now.duration_since(self.last_activity) >= idle {
            self.lock_now(window, cx);
            return;
        }
        if let Some(until) = self.reveal_until {
            if now >= until {
                self.reveal_until = None;
            }
            cx.notify(); // the countdown changes every second
        }
        if self.clip.as_ref().is_some_and(|c| now >= c.deadline)
            && let Some(clip) = self.clip.take()
        {
            clipboard::clear_if_holds(cx, &clip.text);
        }
        if self.notice.as_ref().is_some_and(|n| now >= n.until) {
            self.notice = None;
            cx.notify();
        }
    }

    // ----- lock and unlock -----

    /// Drop the vault key and every secret held by the UI.
    pub fn lock(&mut self, cx: &mut Context<Self>) {
        if self.vault.is_none() && self.is_locked_screen() && self.busy.is_none() {
            return;
        }
        self.generation += 1;
        self.vault = None;
        self.busy = None;
        self.rows.clear();
        self.import = None;
        self.reveal_until = None;
        self.confirm_delete = false;
        self.editing = None;
        self.filter = None;
        self.notice = None;
        self.form_error = None;
        for f in self.all_fields() {
            f.update(cx, |f, cx| f.clear(cx));
        }
        if let Some(clip) = self.clip.take() {
            clipboard::clear_if_holds(cx, &clip.text);
        }
        if !matches!(self.screen, Screen::Fatal(_)) {
            self.screen = Screen::Unlock;
        }
        cx.notify();
    }

    pub fn lock_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.lock(cx);
        self.focus_first(window, cx);
    }

    pub fn submit_auth(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let setting_up = self.screen == Screen::Setup;
        let password = self.text_of(&self.password, cx);
        if setting_up {
            let confirm = self.text_of(&self.confirm, cx);
            if let Err(e) = check_new_password(&password, &confirm) {
                self.form_error = Some(e.to_string().into());
                cx.notify();
                return;
            }
        } else if password.is_empty() {
            return;
        }
        self.form_error = None;
        self.clear_fields(&[&self.password.clone(), &self.confirm.clone()], cx);
        let store = self.store.clone();
        let label = if setting_up {
            "Creating vault…"
        } else {
            "Unlocking…"
        };
        self.run(
            window,
            label,
            cx,
            move || -> Result<Vault, Error> {
                if setting_up {
                    let vault = Vault::create(&password)?;
                    store.save_vault(&vault.seal()?)?;
                    Ok(vault)
                } else {
                    Vault::open(&store.load_vault()?, &password)
                }
            },
            |this, result, window, cx| match result {
                Ok(vault) => {
                    this.vault = Some(vault);
                    this.touch();
                    this.refresh_rows(cx);
                    this.show(Screen::List, window, cx);
                }
                Err(e) => {
                    this.form_error = Some(e.to_string().into());
                    this.focus_first(window, cx);
                }
            },
        );
    }

    // ----- list -----

    pub fn refresh_rows(&mut self, cx: &App) {
        let Some(vault) = &self.vault else {
            self.rows.clear();
            return;
        };
        self.rows = vault
            .search(self.search.read(cx).text(), self.filter)
            .into_iter()
            .map(|e| Row {
                id: e.id.clone(),
                platform: e.platform.clone().into(),
                subtitle: first_line(&e.notes),
                kind: e.kind,
                updated: e.updated,
            })
            .collect();
        self.total = vault.entries().len();
    }

    pub fn set_filter(&mut self, kind: Option<Kind>, cx: &mut Context<Self>) {
        self.filter = kind;
        self.refresh_rows(cx);
        self.list_scroll
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
        cx.notify();
    }

    pub fn open(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_until = None;
        self.confirm_delete = false;
        self.show(Screen::Detail(id), window, cx);
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        match self.screen.clone() {
            Screen::Detail(_) => {
                self.reveal_until = None;
                self.show(Screen::List, window, cx);
            }
            Screen::Edit => {
                self.clear_fields(
                    &[
                        &self.platform.clone(),
                        &self.value.clone(),
                        &self.notes.clone(),
                    ],
                    cx,
                );
                match self.editing.take() {
                    Some(id) => self.show(Screen::Detail(id), window, cx),
                    None => self.show(Screen::List, window, cx),
                }
            }
            Screen::Settings => {
                self.clear_fields(&self.settings_fields(), cx);
                self.show(Screen::List, window, cx);
            }
            Screen::Import => {
                self.import = None;
                self.show(Screen::Settings, window, cx);
            }
            Screen::List if !self.search.read(cx).text().is_empty() => {
                self.search.update(cx, |f, cx| f.clear(cx));
                self.refresh_rows(cx);
                cx.notify();
            }
            _ => {}
        }
    }

    fn settings_fields(&self) -> [&Entity<TextField>; 6] {
        [
            &self.current_pw,
            &self.new_pw,
            &self.new_pw2,
            &self.backup_pw,
            &self.backup_pw2,
            &self.restore_pw,
        ]
    }

    // ----- reveal and copy -----

    pub fn toggle_reveal(&mut self, cx: &mut Context<Self>) {
        self.reveal_until = match self.reveal_until {
            Some(_) => None,
            None => {
                Some(Instant::now() + Duration::from_secs(u64::from(self.settings.reveal_seconds)))
            }
        };
        cx.notify();
    }

    pub fn copy_entry(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.vault.as_ref().and_then(|v| v.get(id)) else {
            return;
        };
        let text = Zeroizing::new(entry.value.clone());
        clipboard::copy_secret(cx, &text);
        let secs = self.settings.clipboard_clear_seconds;
        self.clip = Some(Clip {
            text,
            deadline: Instant::now() + Duration::from_secs(u64::from(secs)),
        });
        self.say(
            format!("Copied. The clipboard clears in {secs} seconds."),
            false,
            cx,
        );
    }

    // ----- add, edit, delete -----

    pub fn start_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.vault.is_none() || self.is_locked_screen() {
            return;
        }
        self.editing = None;
        self.edit_kind = Kind::ApiKey;
        self.clear_fields(
            &[
                &self.platform.clone(),
                &self.value.clone(),
                &self.notes.clone(),
            ],
            cx,
        );
        self.value.update(cx, |f, cx| f.set_masked(true, cx));
        self.show(Screen::Edit, window, cx);
        window.focus(&self.platform.focus_handle(cx));
    }

    pub fn start_edit(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.vault.as_ref().and_then(|v| v.get(id)).cloned() else {
            return;
        };
        self.editing = Some(entry.id.clone());
        self.edit_kind = entry.kind;
        self.reveal_until = None;
        self.platform
            .update(cx, |f, cx| f.set_text(&entry.platform, cx));
        self.value.update(cx, |f, cx| {
            f.set_text(&entry.value, cx);
            f.set_masked(true, cx);
        });
        self.notes.update(cx, |f, cx| f.set_text(&entry.notes, cx));
        self.show(Screen::Edit, window, cx);
        window.focus(&self.platform.focus_handle(cx));
    }

    pub fn save_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let platform = self.platform.read(cx).text().to_string();
        let notes = self.notes.read(cx).text().to_string();
        let value = self.text_of(&self.value, cx);
        let kind = self.edit_kind;
        let stamp = now();
        let Some(vault) = self.vault.as_mut() else {
            return;
        };
        let result = match &self.editing {
            None => Entry::new(&platform, kind, &value, &notes, stamp).and_then(|e| {
                let id = e.id.clone();
                vault.add(e).map(|_| id)
            }),
            Some(id) => match vault.get(id).cloned() {
                Some(mut e) => {
                    e.platform = platform.trim().to_string();
                    e.kind = kind;
                    e.value = value.to_string();
                    e.notes = notes.trim().to_string();
                    e.updated = stamp;
                    let id = e.id.clone();
                    vault.update(e).map(|_| id)
                }
                None => Err(Error::Format("The secret no longer exists.")),
            },
        };
        match result {
            Ok(id) => {
                self.clear_fields(
                    &[
                        &self.platform.clone(),
                        &self.value.clone(),
                        &self.notes.clone(),
                    ],
                    cx,
                );
                self.editing = None;
                self.save(window, cx, "Saved.", Screen::Detail(id));
            }
            Err(e) => {
                self.form_error = Some(e.to_string().into());
                cx.notify();
            }
        }
    }

    pub fn delete_entry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        if let Some(vault) = self.vault.as_mut()
            && vault.remove(id)
        {
            self.confirm_delete = false;
            self.save(window, cx, "Deleted.", Screen::List);
        }
    }

    /// Encrypt the vault now (fast), write the file in the background, then show `next`.
    fn save(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        message: impl Into<SharedString>,
        next: Screen,
    ) {
        let Some(vault) = &self.vault else {
            return;
        };
        let bytes = match vault.seal() {
            Ok(bytes) => bytes,
            Err(e) => {
                self.say(format!("Could not save: {e}"), true, cx);
                return;
            }
        };
        self.refresh_rows(cx);
        let store = self.store.clone();
        let message = message.into();
        self.run(
            window,
            "Saving…",
            cx,
            move || store.save_vault(&bytes),
            move |this, result, window, cx| {
                match result {
                    Ok(()) => this.say(message, false, cx),
                    Err(e) => this.say(
                        format!("Could not save: {e}. Changes are not on disk yet."),
                        true,
                        cx,
                    ),
                }
                this.show(next, window, cx);
            },
        );
    }

    // ----- settings -----

    pub fn change_settings(
        &mut self,
        update: impl FnOnce(&mut Settings),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        update(&mut self.settings);
        self.settings = self.settings.clamped();
        self.apply_theme(window, cx);
        self.save_settings(cx);
    }

    fn save_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_saving {
            self.settings_dirty = true;
            return;
        }
        self.settings_saving = true;
        let (store, settings) = (self.store.clone(), self.settings);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { store.save_settings(&settings) })
                .await;
            this.update(cx, |this, cx| {
                this.settings_saving = false;
                if let Err(e) = result {
                    this.say(format!("Could not save settings: {e}"), true, cx);
                }
                if std::mem::take(&mut this.settings_dirty) {
                    this.save_settings(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn change_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let (current, new, again) = (
            self.text_of(&self.current_pw, cx),
            self.text_of(&self.new_pw, cx),
            self.text_of(&self.new_pw2, cx),
        );
        if let Err(e) = check_new_password(&new, &again) {
            self.say(e.to_string(), true, cx);
            return;
        }
        let Some(mut vault) = self.vault.take() else {
            return;
        };
        self.clear_fields(
            &[
                &self.current_pw.clone(),
                &self.new_pw.clone(),
                &self.new_pw2.clone(),
            ],
            cx,
        );
        let store = self.store.clone();
        self.run(
            window,
            "Changing password…",
            cx,
            move || {
                let result = (|| {
                    if !vault.verify_password(&current)? {
                        return Err(Error::Password("The current password is wrong."));
                    }
                    // The vault keeps its current key unless the new file is saved.
                    vault.change_password(&new, |bytes| store.save_vault(bytes))
                })();
                (vault, result)
            },
            |this, (vault, result), _, cx| {
                this.vault = Some(vault);
                match result {
                    Ok(()) => this.say("Master password changed.", false, cx),
                    Err(e) => this.say(format!("Password not changed. {e}"), true, cx),
                }
            },
        );
    }

    // ----- backup and import -----

    fn start_dir(&self) -> PathBuf {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| self.store.dir().to_path_buf())
    }

    /// Wait for a native dialog without blocking the UI. Only one dialog can be open.
    /// `then` runs only if the vault was not locked meanwhile, is still unlocked and idle,
    /// and the app still shows the screen that opened the dialog. Otherwise the result
    /// is dropped.
    fn after_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        picked: impl Future<Output = Option<PathBuf>> + 'static,
        then: impl FnOnce(&mut Self, PathBuf, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.dialog_open = true;
        let (generation, screen) = (self.generation, self.screen.clone());
        cx.spawn_in(window, async move |this, cx| {
            let path = picked.await;
            this.update_in(cx, |this, window, cx| {
                this.dialog_open = false;
                let current = this.generation == generation
                    && this.vault.is_some()
                    && this.busy.is_none()
                    && this.screen == screen;
                if let (true, Some(path)) = (current, path) {
                    then(this, path, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn pick_file(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, PathBuf, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let first = async move {
            let paths = picked.await.ok()?.ok()??;
            paths.into_iter().next()
        };
        self.after_dialog(window, cx, first, then);
    }

    pub fn export_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.vault.is_none() || self.dialog_open {
            return;
        }
        let (password, again) = (
            self.text_of(&self.backup_pw, cx),
            self.text_of(&self.backup_pw2, cx),
        );
        if let Err(e) = check_new_password(&password, &again) {
            self.say(e.to_string(), true, cx);
            return;
        }
        let picked = cx.prompt_for_new_path(&self.start_dir(), Some("arcaforty-backup.arcabackup"));
        let path = async move { picked.await.ok()?.ok()? };
        self.after_dialog(window, cx, path, move |this, path, window, cx| {
            this.write_backup(path, password, window, cx)
        });
    }

    fn write_backup(
        &mut self,
        path: PathBuf,
        password: Zeroizing<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Early check so the typed passwords stay in the fields. The store checks again.
        if let Err(e) = self.store.check_backup_path(&path) {
            self.say(format!("Backup failed. {e}"), true, cx);
            return;
        }
        let Some(vault) = self.vault.take() else {
            return;
        };
        self.clear_fields(&[&self.backup_pw.clone(), &self.backup_pw2.clone()], cx);
        let store = self.store.clone();
        self.run(
            window,
            "Exporting backup…",
            cx,
            move || {
                let result = store.export_backup(&vault, &password, &path);
                (vault, result)
            },
            |this, (vault, result), _, cx| {
                this.vault = Some(vault);
                match result {
                    Ok(()) => this.say(
                        "Backup saved. Keep its password: it cannot be recovered.",
                        false,
                        cx,
                    ),
                    Err(e) => this.say(format!("Backup failed. {e}"), true, cx),
                }
            },
        );
    }

    pub fn restore_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.vault.is_none() || self.dialog_open {
            return;
        }
        let password = self.text_of(&self.restore_pw, cx);
        if password.is_empty() {
            self.say("Enter the backup password first.", true, cx);
            return;
        }
        self.pick_file(window, cx, move |this, path, window, cx| {
            this.clear_fields(&[&this.restore_pw.clone()], cx);
            this.run(
                window,
                "Restoring backup…",
                cx,
                move || -> Result<Vec<Entry>, Error> {
                    Vault::read_backup(&read_bounded(&path, MAX_FILE_BYTES)?, &password)
                },
                |this, result, window, cx| match result {
                    Ok(entries) => {
                        this.merge_entries(entries, "Restored", Screen::Settings, window, cx)
                    }
                    Err(e) => this.say(format!("Restore failed. {e}"), true, cx),
                },
            );
        });
    }

    pub fn pick_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.vault.is_none() || self.dialog_open {
            return;
        }
        self.pick_file(window, cx, |this, path, window, cx| {
            let source: SharedString = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into();
            this.run(
                window,
                "Reading file…",
                cx,
                move || -> Result<Parsed, Error> {
                    let bytes = read_bounded(&path, MAX_IMPORT_BYTES)?;
                    let text = Zeroizing::new(
                        String::from_utf8(bytes)
                            .map_err(|_| Error::Format("The file is not UTF-8 text."))?,
                    );
                    let is_csv = path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
                    if is_csv {
                        parse_csv(&text, now())
                    } else {
                        parse_env(&text, now())
                    }
                },
                move |this, result, window, cx| match result {
                    Ok(parsed) if parsed.entries.is_empty() => {
                        let why = parsed
                            .warnings
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "The file has no entries.".into());
                        this.say(format!("Nothing to import. {why}"), true, cx);
                    }
                    Ok(parsed) => {
                        this.import = Some(ImportState {
                            source,
                            chosen: vec![true; parsed.entries.len()],
                            entries: parsed.entries,
                            warnings: parsed.warnings,
                        });
                        this.show(Screen::Import, window, cx);
                    }
                    Err(e) => this.say(format!("Import failed. {e}"), true, cx),
                },
            );
        });
    }

    pub fn toggle_import(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(flag) = self.import.as_mut().and_then(|i| i.chosen.get_mut(index)) {
            *flag = !*flag;
            cx.notify();
        }
    }

    pub fn confirm_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let Some(import) = self.import.take() else {
            return;
        };
        let chosen = import
            .entries
            .into_iter()
            .zip(import.chosen)
            .filter_map(|(e, on)| on.then_some(e))
            .collect();
        self.merge_entries(chosen, "Imported", Screen::List, window, cx);
    }

    fn merge_entries(
        &mut self,
        entries: Vec<Entry>,
        verb: &str,
        next: Screen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.as_mut() else {
            return;
        };
        let report = vault.merge(entries);
        let message = format!(
            "{verb} {} secrets. Skipped {} duplicates or over the limit.",
            report.added, report.skipped
        );
        if report.added == 0 {
            self.say(message, false, cx);
            self.show(next, window, cx);
        } else {
            self.save(window, cx, message, next);
        }
    }
}
