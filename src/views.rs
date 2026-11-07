use crate::app::*;
use crate::theme::{self, Palette};
use crate::widgets::{Tone, badge, button, chip, heading, label};
use arcaforty_core::{Appearance, Kind, Settings, format_age, now};
use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    Render, ScrollWheelEvent, Window, div, img, prelude::*, px, uniform_list,
};
use std::time::Instant;

const COLUMN: f32 = 820.0;
const MASKED_VALUE: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = self.palette(cx);
        let body = match self.screen.clone() {
            Screen::Fatal(message) => self.fatal(message.to_string(), pal).into_any_element(),
            Screen::Setup | Screen::Unlock => self.auth(pal, cx).into_any_element(),
            Screen::List => self.list(pal, cx).into_any_element(),
            Screen::Detail(id) => self.detail(&id, pal, cx).into_any_element(),
            Screen::Edit => self.editor(pal, cx).into_any_element(),
            Screen::Settings => self.settings_view(pal, cx).into_any_element(),
            Screen::Import => self.import_view(pal, cx).into_any_element(),
        };
        div()
            .id("root")
            .key_context("Arcaforty")
            .track_focus(&self.focus)
            .size_full()
            .bg(pal.bg)
            .text_color(pal.ink)
            .font_family(theme::BODY)
            .text_size(px(15.))
            .on_action(cx.listener(|this, _: &Back, window, cx| this.back(window, cx)))
            .on_action(cx.listener(|this, _: &LockNow, window, cx| this.lock_now(window, cx)))
            .on_action(cx.listener(|this, _: &NewSecret, window, cx| this.start_new(window, cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                if this.screen == Screen::List {
                    gpui::Focusable::focus_handle(&this.search, cx).focus(window);
                }
            }))
            .on_action(cx.listener(|_, _: &NextField, window, _| window.focus_next()))
            .on_action(cx.listener(|_, _: &PrevField, window, _| window.focus_prev()))
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, _| this.touch()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| this.touch()),
            )
            .on_scroll_wheel(cx.listener(|this, _: &ScrollWheelEvent, _, _| this.touch()))
            .on_key_down(cx.listener(|this, _, _, _| this.touch()))
            .child(body)
    }
}

/// Content column of limited width, centered in the window.
fn page(content: impl IntoElement) -> Div {
    div().size_full().flex().justify_center().child(
        div()
            .w_full()
            .max_w(px(COLUMN))
            .h_full()
            .flex()
            .flex_col()
            .child(content),
    )
}

impl Root {
    fn fatal(&self, message: String, pal: Palette) -> Div {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .items_center()
            .justify_center()
            .child(heading("Arcaforty", 34))
            .child(div().text_color(pal.danger).child(message))
    }

    /// Busy text, or the latest notice.
    fn status(&self, pal: Palette) -> Div {
        let base = div().flex_1().text_center().text_size(px(13.)).truncate();
        match (&self.busy, &self.notice) {
            (Some(text), _) => base.text_color(pal.subtle).child(*text),
            (None, Some(n)) => base
                .text_color(if n.error { pal.danger } else { pal.subtle })
                .child(n.text.clone()),
            _ => base,
        }
    }

    fn error_line(&self, pal: Palette) -> Option<Div> {
        self.form_error.as_ref().map(|e| {
            div()
                .text_size(px(13.))
                .text_color(pal.danger)
                .child(e.clone())
        })
    }

    fn group(&self, title: &'static str, pal: Palette, field: impl IntoElement) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(label(title, pal))
            .child(field)
    }

    // ----- setup and unlock -----

    fn auth(&self, pal: Palette, cx: &mut Context<Self>) -> Div {
        let setup = self.screen == Screen::Setup;
        let busy = self.busy.is_some();
        let text = self
            .busy
            .unwrap_or(if setup { "Create vault" } else { "Unlock" });
        div().size_full().flex().items_center().justify_center().child(
            div()
                .w(px(360.))
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(
                    div().flex().justify_center().pb(px(8.)).child(
                        img(if pal == Palette::dark() {
                            cx.global::<crate::assets::Wordmarks>().dark.clone()
                        } else {
                            cx.global::<crate::assets::Wordmarks>().light.clone()
                        })
                        .size(px(180.)),
                    ),
                )
                .child(self.group("Master password", pal, self.password.clone()))
                .when(setup, |d| d.child(self.group("Confirm password", pal, self.confirm.clone())))
                .when(setup, |d| {
                    d.child(div().text_size(px(13.)).text_color(pal.subtle).child(
                        "Use at least 10 characters. There is no recovery: if you forget this password, the vault cannot be opened.",
                    ))
                })
                .children(self.error_line(pal))
                .child(
                    button("auth", text, Tone::Primary, !busy, pal)
                        .w_full()
                        .on_click(cx.listener(|this, _, window, cx| this.submit_auth(window, cx))),
                ),
        )
    }

    // ----- list -----

    fn list(&self, pal: Palette, cx: &mut Context<Self>) -> Div {
        let filtering = self.filter.is_some() || !self.search.read(cx).text().is_empty();
        let chips = std::iter::once((None, "All".to_string()))
            .chain(
                Kind::ALL
                    .into_iter()
                    .map(|k| (Some(k), k.plural().to_string())),
            )
            .enumerate()
            .map(|(i, (kind, text))| {
                chip(("filter", i), text, self.filter == kind, pal)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_filter(kind, cx)))
            });
        let body = if self.rows.is_empty() {
            let text = if filtering {
                "No matches."
            } else {
                "No secrets yet. Select New secret to add one."
            };
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(pal.subtle)
                .child(text)
                .into_any_element()
        } else {
            uniform_list(
                "rows",
                self.rows.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range.map(|ix| this.row(ix, pal, cx)).collect::<Vec<_>>()
                }),
            )
            .track_scroll(self.list_scroll.clone())
            .flex_1()
            .into_any_element()
        };
        page(
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(32.))
                        .pt(px(24.))
                        .pb(px(16.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(heading("Arcaforty", 26))
                        .child(
                            button("settings", "Settings", Tone::Ghost, true, pal).on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.section = Section::General;
                                    this.show(Screen::Settings, window, cx);
                                }),
                            ),
                        ),
                )
                .child(
                    div()
                        .px(px(32.))
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .child(div().flex_1().child(self.search.clone()))
                        .when(filtering, |d| {
                            d.child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(pal.subtle)
                                    .child(format!("{} of {}", self.rows.len(), self.total)),
                            )
                        }),
                )
                .child(
                    div()
                        .px(px(32.))
                        .py(px(14.))
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .children(chips),
                )
                .child(
                    div()
                        .px(px(24.))
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(body),
                )
                .child(
                    div()
                        .px(px(32.))
                        .py(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .border_t_1()
                        .border_color(pal.line)
                        .child(
                            button("lock", "Lock vault", Tone::Secondary, true, pal).on_click(
                                cx.listener(|this, _, window, cx| this.lock_now(window, cx)),
                            ),
                        )
                        .child(self.status(pal))
                        .child(
                            button("new", "New secret", Tone::Primary, self.busy.is_none(), pal)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.start_new(window, cx)),
                                ),
                        ),
                ),
        )
    }

    fn row(&self, ix: usize, pal: Palette, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.rows[ix];
        let (open_id, copy_id) = (row.id.clone(), row.id.clone());
        let initial = row
            .platform
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default();
        div()
            .id(("row", ix))
            .w_full()
            .h(px(68.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(pal.line)
            .hover(move |s| s.bg(pal.paper))
            .child(
                div()
                    .id(("open", ix))
                    .tab_index(0)
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .cursor_pointer()
                    .focus(move |s| s.bg(pal.paper))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open(open_id.clone(), window, cx)
                    }))
                    .child(
                        div()
                            .size(px(40.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .bg(pal.hover)
                            .font_family(theme::HEADING)
                            .text_size(px(18.))
                            .child(initial),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(row.platform.clone()),
                            )
                            .child(
                                div()
                                    .font_family(theme::MONO)
                                    .text_size(px(13.))
                                    .text_color(pal.subtle)
                                    .truncate()
                                    .child(row.subtitle.clone()),
                            ),
                    )
                    .child(badge(row.kind.label(), pal))
                    .child(
                        div()
                            .w(px(84.))
                            .flex_none()
                            .text_right()
                            .text_size(px(13.))
                            .text_color(pal.subtle)
                            .child(format_age(row.updated, now())),
                    ),
            )
            .child(
                button(("copy", ix), "Copy", Tone::Secondary, true, pal)
                    .h(px(30.))
                    .px(px(12.))
                    .on_click(cx.listener(move |this, _, _, cx| this.copy_entry(&copy_id, cx))),
            )
            .into_any_element()
    }

    // ----- detail -----

    fn detail(&self, id: &str, pal: Palette, cx: &mut Context<Self>) -> Div {
        let Some(entry) = self.vault.as_ref().and_then(|v| v.get(id)) else {
            return div();
        };
        let revealed = self.reveal_until.is_some();
        let remaining = self.reveal_until.map_or(0, |t| {
            t.saturating_duration_since(Instant::now()).as_secs() + 1
        });
        let enabled = self.busy.is_none();
        let (copy_id, edit_id, delete_id) = (id.to_string(), id.to_string(), id.to_string());
        let value = if revealed {
            entry.value.clone()
        } else {
            MASKED_VALUE.to_string()
        };

        let delete_row = if self.confirm_delete {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(pal.subtle)
                        .child("Delete this secret?"),
                )
                .child(
                    button("cancel-delete", "Cancel", Tone::Secondary, true, pal).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.confirm_delete = false;
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button("confirm-delete", "Delete", Tone::Danger, enabled, pal).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.delete_entry(&delete_id, window, cx)
                        }),
                    ),
                )
        } else {
            div().child(
                button("delete", "Delete", Tone::Danger, enabled, pal).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.confirm_delete = true;
                        cx.notify();
                    },
                )),
            )
        };

        page(
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div().px(px(24.)).pt(px(20.)).flex().child(
                        button("back", "\u{2190} All secrets", Tone::Ghost, true, pal)
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                    ),
                )
                .child(
                    div()
                        .px(px(32.))
                        .pt(px(12.))
                        .flex()
                        .flex_col()
                        .gap(px(20.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(14.))
                                .child(heading(entry.platform.clone(), 32).min_w_0().truncate())
                                .child(badge(entry.kind.label(), pal)),
                        )
                        .child(
                            self.group(
                                "Secret",
                                pal,
                                div()
                                    .w_full()
                                    .min_h(px(56.))
                                    .px(px(14.))
                                    .py(px(14.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(pal.line)
                                    .bg(pal.paper)
                                    .font_family(theme::MONO)
                                    .text_size(px(14.))
                                    .text_color(if revealed { pal.ink } else { pal.subtle })
                                    .child(value),
                            ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    button(
                                        "reveal",
                                        if revealed {
                                            format!("Hide ({remaining} s)")
                                        } else {
                                            "Reveal".to_string()
                                        },
                                        Tone::Secondary,
                                        true,
                                        pal,
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_reveal(cx))),
                                )
                                .child(
                                    button("copy-detail", "Copy", Tone::Primary, true, pal)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.copy_entry(&copy_id, cx)
                                        })),
                                )
                                .child(div().flex_1())
                                .child(
                                    button("edit", "Edit", Tone::Secondary, enabled, pal).on_click(
                                        cx.listener(move |this, _, window, cx| {
                                            this.start_edit(&edit_id, window, cx)
                                        }),
                                    ),
                                )
                                .child(delete_row),
                        )
                        .when(!entry.notes.is_empty(), |d| {
                            d.child(self.group(
                                "Notes",
                                pal,
                                div().text_color(pal.ink).child(entry.notes.clone()),
                            ))
                        })
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(pal.subtle)
                                .child(format!("Updated {}", format_age(entry.updated, now()))),
                        ),
                )
                .child(div().flex_1())
                .child(div().px(px(32.)).py(px(14.)).flex().child(self.status(pal))),
        )
    }

    // ----- editor -----

    fn editor(&self, pal: Palette, cx: &mut Context<Self>) -> Div {
        let kinds = Kind::ALL.into_iter().enumerate().map(|(i, kind)| {
            chip(("kind", i), kind.label(), self.edit_kind == kind, pal).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.edit_kind = kind;
                    cx.notify();
                },
            ))
        });
        let masked = self.value.read(cx).is_masked();
        let title = if self.editing.is_some() {
            "Edit secret"
        } else {
            "New secret"
        };
        let busy = self.busy.is_some();
        page(
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("editor")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px(px(32.))
                        .py(px(28.))
                        .flex()
                        .flex_col()
                        .gap(px(20.))
                        .child(heading(title, 30))
                        .child(self.group("Platform", pal, self.platform.clone()))
                        .child(self.group(
                            "Kind",
                            pal,
                            div().flex().flex_wrap().gap(px(8.)).children(kinds),
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(label("Secret", pal))
                                        .child(
                                            div()
                                                .id("toggle-mask")
                                                .tab_index(0)
                                                .cursor_pointer()
                                                .text_size(px(13.))
                                                .text_color(pal.accent)
                                                .child(if masked { "Show" } else { "Hide" })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.value.update(cx, |f, cx| {
                                                        let masked = f.is_masked();
                                                        f.set_masked(!masked, cx);
                                                    });
                                                })),
                                        ),
                                )
                                .child(self.value.clone()),
                        )
                        .child(self.group("Notes", pal, self.notes.clone()))
                        .children(self.error_line(pal)),
                )
                .child(
                    div()
                        .px(px(32.))
                        .py(px(14.))
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(8.))
                        .border_t_1()
                        .border_color(pal.line)
                        .child(self.status(pal))
                        .child(
                            button("cancel", "Cancel", Tone::Secondary, !busy, pal)
                                .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                        )
                        .child(
                            button("save", "Save secret", Tone::Primary, !busy, pal).on_click(
                                cx.listener(|this, _, window, cx| this.save_entry(window, cx)),
                            ),
                        ),
                ),
        )
    }

    // ----- settings -----

    fn options<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        items: Vec<(T, String)>,
        current: T,
        set: fn(&mut Settings, T),
        pal: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .children(items.into_iter().enumerate().map(|(i, (value, text))| {
                chip((id, i), text, value == current, pal).on_click(cx.listener(
                    move |this, _, window, cx| this.change_settings(|s| set(s, value), window, cx),
                ))
            }))
    }

    fn settings_view(&self, pal: Palette, cx: &mut Context<Self>) -> Div {
        let content = match self.section {
            Section::General => self.general_section(pal, cx),
            Section::Security => self.security_section(pal, cx),
            Section::Data => self.data_section(pal, cx),
        };
        let sections = [
            (Section::General, "General"),
            (Section::Security, "Security"),
            (Section::Data, "Backup and import"),
        ];
        let sidebar = sections
            .into_iter()
            .enumerate()
            .map(|(i, (section, text))| {
                let selected = self.section == section;
                div()
                    .id(("section", i))
                    .tab_index(0)
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(14.))
                    .when(selected, |d| {
                        d.bg(pal.hover)
                            .text_color(pal.accent)
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!selected, |d| {
                        d.text_color(pal.ink).hover(move |s| s.bg(pal.hover))
                    })
                    .child(text)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        cx.notify();
                    }))
            });
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .w(px(220.))
                    .flex_none()
                    .h_full()
                    .px(px(16.))
                    .py(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .border_r_1()
                    .border_color(pal.line)
                    .child(heading("Settings", 24).px(px(12.)).pb(px(14.)))
                    .children(sidebar)
                    .child(div().flex_1())
                    .child(
                        button(
                            "close-settings",
                            "\u{2190} Back to vault",
                            Tone::Ghost,
                            true,
                            pal,
                        )
                        .justify_start()
                        .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                    ),
            )
            .child(
                div()
                    .id("settings-content")
                    .flex_1()
                    .h_full()
                    .overflow_y_scroll()
                    .px(px(40.))
                    .py(px(28.))
                    .flex()
                    .flex_col()
                    .gap(px(22.))
                    .max_w(px(640.))
                    .child(content)
                    .child(div().flex().child(self.status(pal).text_left())),
            )
    }

    fn general_section(&self, pal: Palette, cx: &mut Context<Self>) -> Div {
        let s = self.settings;
        let appearance = vec![
            (Appearance::System, "System".to_string()),
            (Appearance::Light, "Light".to_string()),
            (Appearance::Dark, "Dark".to_string()),
        ];
        let minutes = Settings::IDLE_MINUTES
            .iter()
            .map(|&m| (m, format!("{m} min")))
            .collect();
        let clipboard = Settings::CLIPBOARD_SECONDS
            .iter()
            .map(|&n| (n, format!("{n} s")))
            .collect();
        let reveal = Settings::REVEAL_SECONDS
            .iter()
            .map(|&n| (n, format!("{n} s")))
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(self.group(
                "Appearance",
                pal,
                self.options(
                    "appearance",
                    appearance,
                    s.appearance,
                    |s, v| s.appearance = v,
                    pal,
                    cx,
                ),
            ))
            .child(self.group(
                "Lock after inactivity",
                pal,
                self.options(
                    "idle",
                    minutes,
                    s.idle_lock_minutes,
                    |s, v| s.idle_lock_minutes = v,
                    pal,
                    cx,
                ),
            ))
            .child(self.group(
                "Clear clipboard after copy",
                pal,
                self.options(
                    "clip",
                    clipboard,
                    s.clipboard_clear_seconds,
                    |s, v| s.clipboard_clear_seconds = v,
                    pal,
                    cx,
                ),
            ))
            .child(self.group(
                "Hide revealed secrets after",
                pal,
                self.options(
                    "reveal",
                    reveal,
                    s.reveal_seconds,
                    |s, v| s.reveal_seconds = v,
                    pal,
                    cx,
                ),
            ))
    }

    fn security_section(&self, pal: Palette, cx: &mut Context<Self>) -> Div {
        let enabled = self.busy.is_none();
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(heading("Change master password", 20))
            .child(self.group("Current password", pal, self.current_pw.clone()))
            .child(self.group("New password", pal, self.new_pw.clone()))
            .child(self.group("Confirm new password", pal, self.new_pw2.clone()))
            .child(
                div().flex().child(
                    button(
                        "change-password",
                        "Change password",
                        Tone::Primary,
                        enabled,
                        pal,
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.change_password(window, cx))),
                ),
            )
    }

    fn data_section(&self, pal: Palette, cx: &mut Context<Self>) -> Div {
        let enabled = self.busy.is_none();
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(heading("Export backup", 20))
            .child(self.group("Backup password", pal, self.backup_pw.clone()))
            .child(self.group("Confirm backup password", pal, self.backup_pw2.clone()))
            .child(
                div().flex().child(
                    button(
                        "export",
                        "Export backup\u{2026}",
                        Tone::Primary,
                        enabled,
                        pal,
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.export_backup(window, cx))),
                ),
            )
            .child(heading("Restore backup", 20).pt(px(14.)))
            .child(self.group("Backup password", pal, self.restore_pw.clone()))
            .child(
                div().flex().child(
                    button(
                        "restore",
                        "Restore backup\u{2026}",
                        Tone::Secondary,
                        enabled,
                        pal,
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.restore_backup(window, cx))),
                ),
            )
            .child(heading("Import", 20).pt(px(14.)))
            .child(
                div().flex().child(
                    button(
                        "import",
                        ".env or CSV file\u{2026}",
                        Tone::Secondary,
                        enabled,
                        pal,
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.pick_import(window, cx))),
                ),
            )
            .child(
                div()
                    .pt(px(14.))
                    .text_size(px(13.))
                    .text_color(pal.subtle)
                    .child(format!(
                        "Vault file: {}",
                        self.store.dir().join("vault.arca").display()
                    )),
            )
    }

    // ----- import review -----

    fn import_view(&self, pal: Palette, cx: &mut Context<Self>) -> Div {
        let Some(import) = &self.import else {
            return div();
        };
        let selected = import.chosen.iter().filter(|c| **c).count();
        let skipped = import.warnings.len();
        let enabled = self.busy.is_none();
        page(
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(32.))
                        .pt(px(28.))
                        .pb(px(14.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(heading("Review import", 30))
                        .child(div().text_color(pal.subtle).child(format!(
                            "{} secrets found in {}.{}",
                            import.entries.len(),
                            import.source,
                            match skipped {
                                0 => String::new(),
                                1 => " 1 line was skipped.".to_string(),
                                n => format!(" {n} lines were skipped."),
                            }
                        )))
                        .children(import.warnings.iter().take(3).map(|w| {
                            div()
                                .text_size(px(13.))
                                .text_color(pal.muted)
                                .child(w.clone())
                        })),
                )
                .child(
                    div()
                        .px(px(24.))
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(
                            uniform_list(
                                "import-rows",
                                import.entries.len(),
                                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                    range
                                        .map(|ix| this.import_row(ix, pal, cx))
                                        .collect::<Vec<_>>()
                                }),
                            )
                            .track_scroll(self.import_scroll.clone())
                            .flex_1(),
                        ),
                )
                .child(
                    div()
                        .px(px(32.))
                        .py(px(14.))
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(8.))
                        .border_t_1()
                        .border_color(pal.line)
                        .child(
                            button("import-cancel", "Cancel", Tone::Secondary, true, pal)
                                .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                        )
                        .child(
                            button(
                                "import-confirm",
                                format!("Import {selected} selected"),
                                Tone::Primary,
                                enabled && selected > 0,
                                pal,
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.confirm_import(window, cx)),
                            ),
                        ),
                ),
        )
    }

    fn import_row(&self, ix: usize, pal: Palette, cx: &mut Context<Self>) -> AnyElement {
        let import = self.import.as_ref().expect("import state");
        let entry = &import.entries[ix];
        let on = import.chosen[ix];
        div()
            .id(("import", ix))
            .tab_index(0)
            .w_full()
            .h(px(56.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(14.))
            .border_b_1()
            .border_color(pal.line)
            .cursor_pointer()
            .hover(move |s| s.bg(pal.paper))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_import(ix, cx)))
            .child(
                div()
                    .size(px(20.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if on { pal.accent } else { pal.line })
                    .bg(if on { pal.accent } else { pal.paper })
                    .text_color(pal.on_accent)
                    .text_size(px(13.))
                    .child(if on { "\u{2713}" } else { "" }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .child(entry.platform.clone()),
            )
            .child(badge(entry.kind.label(), pal))
            .into_any_element()
    }
}
