use crate::theme::{self, Palette};
use gpui::{Div, ElementId, SharedString, Stateful, div, prelude::*, px};

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tone: Tone,
    enabled: bool,
    pal: Palette,
) -> Stateful<Div> {
    let (bg, fg, border) = match tone {
        Tone::Primary => (pal.accent, pal.on_accent, pal.accent),
        Tone::Secondary => (pal.paper, pal.ink, pal.line),
        Tone::Danger => (pal.paper, pal.danger, pal.danger),
        Tone::Ghost => (
            gpui::transparent_black(),
            pal.subtle,
            gpui::transparent_black(),
        ),
    };
    div()
        .id(id)
        .tab_index(0)
        .flex()
        .items_center()
        .justify_center()
        .h(px(36.))
        .px(px(16.))
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .text_size(px(14.))
        .font_family(theme::BODY)
        .whitespace_nowrap()
        .focus(move |s| s.border_color(pal.ink))
        .when(enabled, |b| {
            b.cursor_pointer().hover(move |s| match tone {
                Tone::Primary => s.bg(pal.accent.opacity(0.88)),
                _ => s.bg(pal.hover),
            })
        })
        .when(!enabled, |b| b.opacity(0.5))
        .child(label.into())
}

pub fn chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    pal: Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .tab_index(0)
        .flex()
        .items_center()
        .h(px(32.))
        .px(px(12.))
        .rounded_full()
        .border_1()
        .text_size(px(14.))
        .whitespace_nowrap()
        .cursor_pointer()
        .focus(move |s| s.border_color(pal.ink))
        .when(selected, |c| {
            c.bg(pal.accent)
                .border_color(pal.accent)
                .text_color(pal.on_accent)
        })
        .when(!selected, |c| {
            c.bg(pal.paper)
                .border_color(pal.line)
                .text_color(pal.ink)
                .hover(move |s| s.bg(pal.hover))
        })
        .child(label.into())
}

pub fn heading(text: impl Into<SharedString>, size: u32) -> Div {
    let size = size as f32;
    div()
        .font_family(theme::HEADING)
        .text_size(px(size))
        .line_height(px(size * 1.2))
        .child(text.into())
}

pub fn label(text: impl Into<SharedString>, pal: Palette) -> Div {
    div()
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(pal.subtle)
        .child(text.into())
}

pub fn badge(text: impl Into<SharedString>, pal: Palette) -> Div {
    div()
        .px(px(8.))
        .py(px(2.))
        .rounded(px(4.))
        .border_1()
        .border_color(pal.line)
        .text_size(px(12.))
        .text_color(pal.subtle)
        .whitespace_nowrap()
        .child(text.into())
}
