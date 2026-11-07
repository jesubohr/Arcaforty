#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod clipboard;
mod field;
mod theme;
mod views;
mod widgets;

use app::{Back, FocusSearch, LockNow, NewSecret, NextField, PrevField, Root};
use gpui::{
    App, Application, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, prelude::*,
    px, size,
};

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.set_global(assets::Wordmarks::load());
        field::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("tab", NextField, None),
            KeyBinding::new("shift-tab", PrevField, None),
            KeyBinding::new("escape", Back, None),
            KeyBinding::new("ctrl-l", LockNow, None),
            KeyBinding::new("ctrl-n", NewSecret, None),
            KeyBinding::new("ctrl-f", FocusSearch, None),
        ]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(940.), px(700.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(720.), px(520.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Arcaforty".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let window = cx.open_window(options, |window, cx| cx.new(|cx| Root::new(window, cx)));
        if window.is_err() {
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
