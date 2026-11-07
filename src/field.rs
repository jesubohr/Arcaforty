//! Text input for GPUI: single or multi line, optional masking. Masked fields never show
//! their text and refuse copy and cut.

use crate::theme::{self, Palette};
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId,
    InspectorElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, ScrollWheelEvent, SharedString, Style, TextAlign,
    TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine, actions, div, fill, point,
    prelude::*, px, relative, size,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use zeroize::Zeroize;

actions!(
    field,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        Home,
        End,
        SelectHome,
        SelectEnd,
        SelectAll,
        CopyText,
        CutText,
        PasteText,
        Enter
    ]
);

const LINE_HEIGHT: f32 = 22.0;
const MASK: char = '\u{2022}';
const MASK_LEN: usize = MASK.len_utf8();

pub fn bind_keys(cx: &mut App) {
    let ctx = Some("TextField");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("shift-up", SelectUp, ctx),
        KeyBinding::new("shift-down", SelectDown, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("shift-home", SelectHome, ctx),
        KeyBinding::new("shift-end", SelectEnd, ctx),
        KeyBinding::new("ctrl-a", SelectAll, ctx),
        KeyBinding::new("ctrl-c", CopyText, ctx),
        KeyBinding::new("ctrl-x", CutText, ctx),
        KeyBinding::new("ctrl-v", PasteText, ctx),
        KeyBinding::new("enter", Enter, ctx),
    ]);
}

/// Emitted when Enter is pressed in a single-line field.
pub struct Submit;
/// Emitted after every edit.
pub struct Changed;

pub struct TextField {
    focus: FocusHandle,
    content: String,
    placeholder: SharedString,
    multiline: bool,
    mono: bool,
    masked: bool,
    max_len: usize,
    rows: usize,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    selecting: bool,
    scroll: Point<Pixels>,
    reveal_cursor: bool,
    // Filled while painting.
    lines: Vec<WrappedLine>,
    bounds: Bounds<Pixels>,
    content_height: Pixels,
}

impl EventEmitter<Submit> for TextField {}
impl EventEmitter<Changed> for TextField {}

impl Drop for TextField {
    fn drop(&mut self) {
        self.content.zeroize();
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TextField {
    pub fn new(cx: &mut Context<Self>, placeholder: &str, max_len: usize) -> Self {
        TextField {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            // Full capacity up front, so edits never move the text to a new buffer.
            content: String::with_capacity(max_len + 4),
            placeholder: placeholder.to_string().into(),
            multiline: false,
            mono: false,
            masked: false,
            max_len,
            rows: 1,
            selected: 0..0,
            reversed: false,
            marked: None,
            selecting: false,
            scroll: Point::default(),
            reveal_cursor: false,
            lines: Vec::new(),
            bounds: Bounds::default(),
            content_height: Pixels::ZERO,
        }
    }

    pub fn masked(mut self) -> Self {
        self.masked = true;
        self
    }

    pub fn multiline(mut self, rows: usize) -> Self {
        self.multiline = true;
        self.rows = rows;
        self
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn is_masked(&self) -> bool {
        self.masked
    }

    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        self.masked = masked;
        cx.notify();
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.clear(cx);
        self.insert(0..0, text, cx);
    }

    /// Erase the text from memory.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content.zeroize();
        self.selected = 0..0;
        self.reversed = false;
        self.marked = None;
        self.scroll = Point::default();
        self.lines.clear();
        cx.notify();
    }

    // ----- editing -----

    fn insert(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let text = self.clean(text, range.len());
        self.content.replace_range(range.clone(), &text);
        let end = range.start + text.len();
        self.selected = end..end;
        self.reversed = false;
        self.marked = None;
        self.reveal_cursor = true;
        cx.emit(Changed);
        cx.notify();
    }

    /// Normalize line breaks and cut the text to fit the length limit.
    fn clean(&self, text: &str, replaced: usize) -> String {
        let mut out: String = if self.multiline {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            text.chars()
                .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                .collect()
        };
        let room = self.max_len.saturating_sub(self.content.len() - replaced);
        if out.len() > room {
            let mut cut = room;
            while !out.is_char_boundary(cut) {
                cut -= 1;
            }
            out.truncate(cut);
        }
        out
    }

    fn delete_selection_or(&mut self, from: usize, cx: &mut Context<Self>) {
        let range = if self.selected.is_empty() {
            from.min(self.cursor())..from.max(self.cursor())
        } else {
            self.selected.clone()
        };
        if !range.is_empty() {
            self.insert(range, "", cx);
        }
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_selection_or(self.prev_boundary(self.cursor()), cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_selection_or(self.next_boundary(self.cursor()), cx);
    }

    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.insert(self.selected.clone(), "\n", cx);
        } else {
            cx.emit(Submit);
        }
    }

    fn paste(&mut self, _: &PasteText, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert(self.selected.clone(), &text, cx);
        }
    }

    fn copy(&mut self, _: &CopyText, _: &mut Window, cx: &mut Context<Self>) {
        if !self.masked && !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &CutText, window: &mut Window, cx: &mut Context<Self>) {
        if !self.masked && !self.selected.is_empty() {
            self.copy(&CopyText, window, cx);
            self.insert(self.selected.clone(), "", cx);
        }
    }

    // ----- cursor and selection -----

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected = offset..offset;
        self.reversed = false;
        self.reveal_cursor = true;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        self.reveal_cursor = true;
        cx.notify();
    }

    fn prev_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(i, _)| (i < offset).then_some(i))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(i, _)| (i > offset).then_some(i))
            .unwrap_or(self.content.len())
    }

    fn line_start(&self, offset: usize) -> usize {
        self.content[..offset].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self, offset: usize) -> usize {
        self.content[offset..]
            .find('\n')
            .map_or(self.content.len(), |i| offset + i)
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let to = if self.selected.is_empty() {
            self.prev_boundary(self.cursor())
        } else {
            self.selected.start
        };
        self.move_to(to, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let to = if self.selected.is_empty() {
            self.next_boundary(self.cursor())
        } else {
            self.selected.end
        };
        self.move_to(to, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.prev_boundary(self.cursor()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor()), cx);
    }

    fn up_target(&self) -> usize {
        self.vertical_target(-1.0, 0)
    }

    fn down_target(&self) -> usize {
        self.vertical_target(1.0, self.content.len())
    }

    /// One visual row up or down. Single-line fields jump to the start or end.
    fn vertical_target(&self, rows: f32, edge: usize) -> usize {
        if !self.multiline {
            return edge;
        }
        let lh = px(LINE_HEIGHT);
        match locate(&self.lines, self.display_offset(self.cursor()), lh) {
            Some(p) => {
                let target = point(p.x, p.y + lh * rows + lh * 0.5);
                if target.y < Pixels::ZERO {
                    0
                } else {
                    self.offset_at_local(target)
                }
            }
            None => edge,
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.up_target(), cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.down_target(), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.up_target(), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.down_target(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_start(self.cursor()), cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_end(self.cursor()), cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_start(self.cursor()), cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_end(self.cursor()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected = 0..self.content.len();
        self.reversed = false;
        cx.notify();
    }

    // ----- mouse -----

    fn mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = true;
        let offset = self.offset_at(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.offset_at(event.position), cx);
        }
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            let dy = event.delta.pixel_delta(px(LINE_HEIGHT)).y;
            let max = (self.content_height - self.bounds.size.height).max(Pixels::ZERO);
            self.scroll.y = (self.scroll.y - dy).clamp(Pixels::ZERO, max);
            cx.notify();
        }
    }

    // ----- text positions -----

    /// Offsets in the shaped text differ from content offsets only when masked.
    fn display_offset(&self, offset: usize) -> usize {
        if !self.masked {
            return offset;
        }
        self.content[..offset]
            .chars()
            .map(|c| if c == '\n' { 1 } else { MASK_LEN })
            .sum()
    }

    fn content_offset(&self, display: usize) -> usize {
        if !self.masked {
            return display.min(self.content.len());
        }
        let mut seen = 0;
        for (i, c) in self.content.char_indices() {
            if seen >= display {
                return i;
            }
            seen += if c == '\n' { 1 } else { MASK_LEN };
        }
        self.content.len()
    }

    fn display_text(&self) -> SharedString {
        if self.masked {
            self.content
                .chars()
                .map(|c| if c == '\n' { c } else { MASK })
                .collect::<String>()
                .into()
        } else {
            self.content.clone().into()
        }
    }

    fn offset_at(&self, position: Point<Pixels>) -> usize {
        self.offset_at_local(position - self.bounds.origin + self.scroll)
    }

    /// `local` is relative to the top left of the text, scroll included.
    fn offset_at_local(&self, local: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let lh = px(LINE_HEIGHT);
        let (mut y, mut start) = (Pixels::ZERO, 0);
        for line in &self.lines {
            let height = lh * (line.wrap_boundaries.len() + 1) as f32;
            if local.y < y + height {
                let at = point(local.x, (local.y - y).max(Pixels::ZERO));
                let index = line
                    .closest_index_for_position(at, lh)
                    .unwrap_or_else(|e| e);
                return self.content_offset(start + index);
            }
            y += height;
            start += line.len() + 1;
        }
        self.content.len()
    }

    // ----- UTF-16 conversion for the input method -----

    fn to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].encode_utf16().count()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = (0, 0);
        for c in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += c.len_utf16();
            utf8 += c.len_utf8();
        }
        utf8
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end)
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }
}

/// Position of a display offset inside the text block, or None if out of range.
fn locate(lines: &[WrappedLine], display: usize, lh: Pixels) -> Option<Point<Pixels>> {
    let (mut y, mut start) = (Pixels::ZERO, 0);
    for line in lines {
        if display <= start + line.len() {
            let p = line.position_for_index(display - start, lh)?;
            return Some(point(p.x, y + p.y));
        }
        y += lh * (line.wrap_boundaries.len() + 1) as f32;
        start += line.len() + 1;
    }
    None
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        if self.masked {
            return None;
        }
        let range = self.range_from_utf16(&range);
        actual.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| self.range_from_utf16(&r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected.clone());
        self.insert(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        new_selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| self.range_from_utf16(&r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected.clone());
        let start = range.start;
        self.insert(range, text, cx);
        let end = self.selected.end;
        self.marked = (end > start).then_some(start..end);
        if let Some(sel) = new_selection {
            let base = self.to_utf16(start);
            let abs = base + sel.start..base + sel.end;
            self.selected = self.range_from_utf16(&abs);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        element: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let start = self.range_from_utf16(&range).start;
        let p = locate(&self.lines, self.display_offset(start), px(LINE_HEIGHT))?;
        Some(Bounds::new(
            element.origin + p - self.scroll,
            size(px(1.), px(LINE_HEIGHT)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.to_utf16(self.offset_at(p)))
    }
}

// ----- element -----

struct TextElement {
    field: Entity<TextField>,
}

struct Prepaint {
    lines: Vec<WrappedLine>,
    selection: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    scroll: Point<Pixels>,
}

impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let rows = self.field.read(cx).rows;
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (px(LINE_HEIGHT) * rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let pal = *cx.global::<Palette>();
        let f = self.field.read(cx);
        let lh = px(LINE_HEIGHT);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());

        let empty = f.content.is_empty();
        let (text, color) = if empty {
            (f.placeholder.clone(), pal.muted)
        } else {
            (f.display_text(), pal.ink)
        };
        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match f.marked.as_ref().filter(|_| !empty) {
            Some(m) => {
                let (s, e) = (f.display_offset(m.start), f.display_offset(m.end));
                let underline = UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.),
                    wavy: false,
                };
                [
                    TextRun {
                        len: s,
                        ..run.clone()
                    },
                    TextRun {
                        len: e - s,
                        underline: Some(underline),
                        ..run.clone()
                    },
                    TextRun {
                        len: text.len() - e,
                        ..run
                    },
                ]
                .into_iter()
                .filter(|r| r.len > 0)
                .collect()
            }
            None => vec![run],
        };
        let wrap = f.multiline.then_some(bounds.size.width);
        let lines: Vec<WrappedLine> = window
            .text_system()
            .shape_text(text, font_size, &runs, wrap, None)
            .map(|l| l.into_vec())
            .unwrap_or_default();

        let total = lines.iter().fold(Pixels::ZERO, |acc, l| {
            acc + lh * (l.wrap_boundaries.len() + 1) as f32
        });
        let cursor_display = f.display_offset(f.cursor());
        let cursor_at = locate(&lines, cursor_display, lh).unwrap_or_default();

        // Keep the cursor visible after edits and moves, and keep the scroll in range.
        let mut scroll = f.scroll;
        if f.reveal_cursor {
            if f.multiline {
                if cursor_at.y < scroll.y {
                    scroll.y = cursor_at.y;
                } else if cursor_at.y + lh > scroll.y + bounds.size.height {
                    scroll.y = cursor_at.y + lh - bounds.size.height;
                }
            } else if cursor_at.x < scroll.x {
                scroll.x = cursor_at.x;
            } else if cursor_at.x + px(2.) > scroll.x + bounds.size.width {
                scroll.x = cursor_at.x + px(2.) - bounds.size.width;
            }
        }
        scroll.y = scroll
            .y
            .clamp(Pixels::ZERO, (total - bounds.size.height).max(Pixels::ZERO));
        scroll.x = if f.multiline {
            Pixels::ZERO
        } else {
            scroll.x.max(Pixels::ZERO)
        };

        let origin = bounds.origin - scroll;
        let sel = f.display_offset(f.selected.start)..f.display_offset(f.selected.end);
        let selection = if empty || sel.is_empty() {
            Vec::new()
        } else {
            selection_quads(&lines, sel.clone(), lh, origin, bounds.size.width, pal)
        };
        let cursor = (sel.is_empty() || empty).then(|| {
            fill(
                Bounds::new(
                    point(origin.x + cursor_at.x, origin.y + cursor_at.y),
                    size(px(1.5), lh),
                ),
                pal.ink,
            )
        });

        self.field.update(cx, |f, _| {
            f.scroll = scroll;
            f.reveal_cursor = false;
            f.bounds = bounds;
            f.content_height = total;
        });
        Prepaint {
            lines,
            selection,
            cursor,
            scroll,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.field.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );
        let lh = px(LINE_HEIGHT);
        let origin = bounds.origin - prepaint.scroll;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for quad in prepaint.selection.drain(..) {
                window.paint_quad(quad);
            }
            let mut y = Pixels::ZERO;
            for line in &prepaint.lines {
                let _ = line.paint(
                    point(origin.x, origin.y + y),
                    lh,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
                y += lh * (line.wrap_boundaries.len() + 1) as f32;
            }
            if focus.is_focused(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });
        let lines = std::mem::take(&mut prepaint.lines);
        self.field.update(cx, |f, _| f.lines = lines);
    }
}

fn selection_quads(
    lines: &[WrappedLine],
    sel: Range<usize>,
    lh: Pixels,
    origin: Point<Pixels>,
    width: Pixels,
    pal: Palette,
) -> Vec<PaintQuad> {
    let mut quads = Vec::new();
    let mut rect = |x0: Pixels, x1: Pixels, y: Pixels| {
        quads.push(fill(
            Bounds::from_corners(
                point(origin.x + x0, origin.y + y),
                point(origin.x + x1, origin.y + y + lh),
            ),
            pal.selection,
        ));
    };
    let (mut y, mut start) = (Pixels::ZERO, 0);
    for line in lines {
        let end = start + line.len();
        let (s, e) = (sel.start.max(start), sel.end.min(end));
        // A selected line break shows as a small extra block.
        let tail = if sel.start <= end && sel.end > end {
            px(5.)
        } else {
            Pixels::ZERO
        };
        if s <= e
            && (s < e || tail > Pixels::ZERO)
            && let (Some(p0), Some(p1)) = (
                line.position_for_index(s - start, lh),
                line.position_for_index(e - start, lh),
            )
        {
            if p0.y == p1.y {
                rect(p0.x, p1.x + tail, y + p0.y);
            } else {
                rect(p0.x, width, y + p0.y);
                let mut row = p0.y + lh;
                while row < p1.y {
                    rect(Pixels::ZERO, width, y + row);
                    row += lh;
                }
                rect(Pixels::ZERO, p1.x + tail, y + p1.y);
            }
        }
        y += lh * (line.wrap_boundaries.len() + 1) as f32;
        start = end + 1;
    }
    quads
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = *cx.global::<Palette>();
        let focused = self.focus.is_focused(window);
        div()
            .key_context("TextField")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::enter))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
            .w_full()
            .px(px(12.))
            .py(px(8.))
            .bg(pal.paper)
            .border_1()
            .border_color(if focused { pal.accent } else { pal.line })
            .rounded(px(6.))
            .font_family(if self.mono { theme::MONO } else { theme::BODY })
            .text_size(px(if self.mono { 14. } else { 15. }))
            .line_height(px(LINE_HEIGHT))
            .text_color(pal.ink)
            .child(TextElement { field: cx.entity() })
    }
}
