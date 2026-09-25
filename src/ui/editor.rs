//! Minimal code editor view for plain diffs: virtualized rows, gutter, caret, selection,
//! keyboard editing, clipboard, undo. Editing rules live in `crate::buffer`.

use crate::align::{Aligned, RowKind, SideRow};
use crate::buffer::{col_from_visual, visual_col, Buffer, Move, Pos};
use crate::theme;
use gpui::{
    actions, div, point, prelude::*, px, uniform_list, App, ClipboardItem, Context, EventEmitter, FocusHandle,
    Focusable, HighlightStyle, KeyBinding, KeyDownEvent, ListHorizontalSizingBehavior, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, Point, ScrollStrategy, SharedString, StyledText, UniformListScrollHandle, Window,
};

const TAB: usize = 4;

actions!(
    kerf_editor,
    [
        Backspace,
        BackspaceWord,
        Delete,
        DeleteWord,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        LineStart,
        LineEnd,
        DocStart,
        DocEnd,
        PageUp,
        PageDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectDocStart,
        SelectDocEnd,
        Newline,
        InsertTab,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Submit,
        Blur,
        DeleteToLineStart,
        DeleteToLineEnd,
        ClearAll,
        DeleteLine,
        SelectLine,
        MoveLineUp,
        MoveLineDown,
        DuplicateLineUp,
        DuplicateLineDown,
        Indent,
        Outdent,
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys(bindings());
}

pub fn bindings() -> Vec<KeyBinding> {
    let c = Some("KerfEditor");
    // Keys that are the same everywhere.
    let mut b = vec![
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("delete", Delete, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("up", Up, c),
        KeyBinding::new("down", Down, c),
        KeyBinding::new("home", LineStart, c),
        KeyBinding::new("end", LineEnd, c),
        KeyBinding::new("pageup", PageUp, c),
        KeyBinding::new("pagedown", PageDown, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("shift-home", SelectLineStart, c),
        KeyBinding::new("shift-end", SelectLineEnd, c),
        KeyBinding::new("enter", Newline, c),
        KeyBinding::new("tab", InsertTab, c),
        KeyBinding::new("shift-tab", Outdent, c),
        KeyBinding::new("escape", Blur, c),
        KeyBinding::new("alt-up", MoveLineUp, c),
        KeyBinding::new("alt-down", MoveLineDown, c),
        KeyBinding::new("alt-shift-up", DuplicateLineUp, c),
        KeyBinding::new("alt-shift-down", DuplicateLineDown, c),
        // secondary = ⌘ on macOS, Ctrl elsewhere.
        KeyBinding::new("secondary-a", SelectAll, c),
        KeyBinding::new("secondary-c", Copy, c),
        KeyBinding::new("secondary-x", Cut, c),
        KeyBinding::new("secondary-v", Paste, c),
        KeyBinding::new("secondary-z", Undo, c),
        KeyBinding::new("secondary-shift-z", Redo, c),
        KeyBinding::new("secondary-enter", Submit, c),
        KeyBinding::new("secondary-shift-backspace", ClearAll, c),
        KeyBinding::new("secondary-shift-k", DeleteLine, c),
        KeyBinding::new("secondary-l", SelectLine, c),
        KeyBinding::new("secondary-]", Indent, c),
        KeyBinding::new("secondary-[", Outdent, c),
    ];
    if cfg!(target_os = "macos") {
        b.extend([
            KeyBinding::new("alt-backspace", BackspaceWord, c),
            KeyBinding::new("alt-delete", DeleteWord, c),
            KeyBinding::new("alt-left", WordLeft, c),
            KeyBinding::new("alt-right", WordRight, c),
            KeyBinding::new("alt-shift-left", SelectWordLeft, c),
            KeyBinding::new("alt-shift-right", SelectWordRight, c),
            KeyBinding::new("cmd-left", LineStart, c),
            KeyBinding::new("cmd-right", LineEnd, c),
            KeyBinding::new("cmd-shift-left", SelectLineStart, c),
            KeyBinding::new("cmd-shift-right", SelectLineEnd, c),
            KeyBinding::new("cmd-up", DocStart, c),
            KeyBinding::new("cmd-down", DocEnd, c),
            KeyBinding::new("cmd-shift-up", SelectDocStart, c),
            KeyBinding::new("cmd-shift-down", SelectDocEnd, c),
            KeyBinding::new("cmd-backspace", DeleteToLineStart, c),
            KeyBinding::new("cmd-delete", DeleteToLineEnd, c),
        ]);
    } else {
        // Linux / Windows editing conventions.
        b.extend([
            KeyBinding::new("ctrl-backspace", BackspaceWord, c),
            KeyBinding::new("ctrl-delete", DeleteWord, c),
            KeyBinding::new("ctrl-left", WordLeft, c),
            KeyBinding::new("ctrl-right", WordRight, c),
            KeyBinding::new("ctrl-shift-left", SelectWordLeft, c),
            KeyBinding::new("ctrl-shift-right", SelectWordRight, c),
            KeyBinding::new("ctrl-home", DocStart, c),
            KeyBinding::new("ctrl-end", DocEnd, c),
            KeyBinding::new("ctrl-shift-home", SelectDocStart, c),
            KeyBinding::new("ctrl-shift-end", SelectDocEnd, c),
            KeyBinding::new("ctrl-y", Redo, c),
        ]);
    }
    b
}

pub enum EditorEvent {
    /// ⌘↵ — the user wants to see the diff.
    Submit,
    /// Text changed.
    Changed,
    /// Esc — give focus back to the app.
    Blur,
}

pub struct Editor {
    pub buffer: Buffer,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    font: SharedString,
    placeholder: SharedString,
    /// Measured monospace advance at `TEXT_CODE`.
    pub char_w: f32,
    dragging: bool,
    /// Live-diff layout for this side: view rows (lines + fillers) with change marks.
    decor: Option<std::sync::Arc<Vec<SideRow>>>,
    row_of_line: Vec<usize>,
    /// Left side tints changes red, right side green.
    is_left: bool,
    /// Rows are at least this wide, so both sides scroll horizontally in lockstep.
    pub min_row_w: f32,
    /// Gutter digits floor, shared with the other side so gutters line up.
    pub min_digits: usize,
    /// This editor's own on-screen bounds (the scroll handle may be shared with another list).
    bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<Pixels>>>,
}

impl EventEmitter<EditorEvent> for Editor {}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Editor {
    /// `scroll` may be shared with the other side's editor to scroll both together.
    pub fn new(
        text: &str,
        font: SharedString,
        placeholder: &str,
        scroll: UniformListScrollHandle,
        is_left: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            buffer: Buffer::new(text),
            focus: cx.focus_handle(),
            scroll,
            font,
            placeholder: placeholder.to_string().into(),
            char_w: 8.4,
            dragging: false,
            decor: None,
            row_of_line: Vec::new(),
            is_left,
            min_row_w: 0.,
            min_digits: 3,
            bounds: Default::default(),
        }
    }

    pub fn set_left(&mut self, is_left: bool) {
        self.is_left = is_left;
    }

    /// Installs the aligned layout from the latest diff (or clears it).
    pub fn set_decor(&mut self, rows: Option<std::sync::Arc<Vec<SideRow>>>, cx: &mut Context<Self>) {
        self.row_of_line = rows.as_ref().map(|r| Aligned::row_of_line(r, self.buffer.line_count())).unwrap_or_default();
        self.decor = rows;
        cx.notify();
    }

    /// Rows this editor draws: aligned view rows when diffing, else buffer lines.
    pub fn view_rows(&self) -> usize {
        self.decor.as_ref().map(|d| d.len()).unwrap_or(self.buffer.line_count())
    }

    fn view_line(&self, view: usize) -> Option<usize> {
        match &self.decor {
            Some(d) => d.get(view).and_then(|r| r.line).filter(|l| *l < self.buffer.line_count()),
            None => (view < self.buffer.line_count()).then_some(view),
        }
    }

    fn line_view_row(&self, line: usize) -> usize {
        if self.decor.is_some() {
            self.row_of_line.get(line).copied().unwrap_or(line)
        } else {
            line
        }
    }

    /// Longest line in chars (tabs expanded).
    pub fn widest_chars(&self) -> usize {
        self.buffer.lines().iter().map(|l| visual_col(l, l.chars().count(), TAB)).max().unwrap_or(0)
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus);
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buffer.set_text(text);
        self.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    pub fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buffer.insert(text);
        self.after_edit(cx);
    }

    fn gutter_w(&self) -> f32 {
        let digits = self.buffer.line_count().to_string().len().max(self.min_digits);
        digits as f32 * self.char_w + 20.
    }

    fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.reveal_caret();
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn after_move(&mut self, cx: &mut Context<Self>) {
        self.reveal_caret();
        cx.notify();
    }

    /// Scrolls so the caret is visible, vertically and horizontally.
    fn reveal_caret(&mut self) {
        let c = self.buffer.caret();
        self.scroll.scroll_to_item(self.line_view_row(c.row), ScrollStrategy::Top);
        let state = self.scroll.0.borrow();
        let base = &state.base_handle;
        let view_w: f32 = self.bounds.get().size.width.into();
        if view_w <= 0. {
            return;
        }
        let off: Point<Pixels> = base.offset();
        let caret_x = self.gutter_w() + visual_col(self.buffer.line(c.row), c.col, TAB) as f32 * self.char_w;
        let left = -f32::from(off.x);
        let margin = self.char_w * 4.;
        let new_left = if caret_x > left + view_w - margin {
            caret_x - view_w + margin
        } else if caret_x < left + self.gutter_w() + margin {
            (caret_x - self.gutter_w() - margin).max(0.)
        } else {
            return;
        };
        base.set_offset(point(px(-new_left), off.y));
    }

    fn mv(&mut self, m: Move, extend: bool, cx: &mut Context<Self>) {
        self.buffer.move_caret(m, extend);
        self.after_move(cx);
    }

    fn page_rows(&self) -> usize {
        let h: f32 = self.scroll.0.borrow().base_handle.bounds().size.height.into();
        ((h / f32::from(theme::ROW_CODE)) as usize).saturating_sub(2).max(1)
    }

    /// Window position → buffer position.
    fn hit(&self, p: Point<Pixels>) -> Pos {
        let state = self.scroll.0.borrow();
        let b = self.bounds.get();
        let off = state.base_handle.offset();
        let y: f32 = (p.y - b.origin.y - off.y).into();
        let view = ((y / f32::from(theme::ROW_CODE)).floor().max(0.) as usize).min(self.view_rows().saturating_sub(1));
        // Filler rows map to the end of the nearest real line above them.
        let row = (0..=view).rev().find_map(|v| self.view_line(v)).unwrap_or(0);
        if self.view_line(view).is_none() {
            return Pos::new(row, self.buffer.line(row).chars().count());
        }
        let x: f32 = (p.x - b.origin.x - off.x).into();
        let vcol = (x - self.gutter_w() - 8.) / self.char_w;
        Pos::new(row, col_from_visual(self.buffer.line(row), vcol.max(0.), TAB))
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let p = self.hit(ev.position);
        if ev.click_count >= 3 {
            self.buffer.set_caret(Pos::new(p.row, 0), false);
            self.buffer.move_caret(Move::LineEnd, true);
        } else if ev.click_count == 2 {
            self.buffer.set_caret(p, false);
            self.buffer.move_caret(Move::WordLeft, false);
            self.buffer.move_caret(Move::WordRight, true);
        } else {
            self.buffer.set_caret(p, ev.modifiers.shift);
            self.dragging = true;
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        if ev.pressed_button != Some(MouseButton::Left) {
            self.dragging = false;
            return;
        }
        let p = self.hit(ev.position);
        self.buffer.set_caret(p, true);
        self.reveal_caret();
        cx.notify();
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if ks.modifiers.platform || ks.modifiers.control {
            return;
        }
        let Some(ch) = ks.key_char.as_ref() else { return };
        if ch.is_empty() || ch.chars().any(char::is_control) {
            return;
        }
        self.buffer.insert(ch);
        self.after_edit(cx);
        cx.stop_propagation();
    }

    fn render_row(&self, view: usize, focused: bool) -> gpui::AnyElement {
        let gutter_w = self.gutter_w();
        let decor = self.decor.as_ref().and_then(|d| d.get(view));
        let Some(ix) = self.view_line(view) else {
            // Filler: the other side has lines here.
            return div()
                .id(view)
                .h(theme::ROW_CODE)
                .min_w(px(self.min_row_w))
                .min_w_full()
                .flex()
                .child(
                    div()
                        .w(px(gutter_w))
                        .h_full()
                        .flex_none()
                        .bg(theme::abyss())
                        .border_r_1()
                        .border_color(theme::line()),
                )
                .child(div().flex_1().h_full().bg(theme::abyss()))
                .into_any_element();
        };
        let kind = decor.map(|d| d.kind).unwrap_or(RowKind::Same);
        let (tint, emph_bg) =
            if self.is_left { (theme::del_bg(), theme::del_emph()) } else { (theme::add_bg(), theme::add_emph()) };
        let line = self.buffer.line(ix);
        let caret = self.buffer.caret();
        let is_caret_row = caret.row == ix;
        let display: String = line.replace('\t', &" ".repeat(TAB));
        let byte = |vc: usize| display.char_indices().nth(vc).map(|(b, _)| b).unwrap_or(display.len());
        let mut hl = Vec::new();
        // Word-level change emphasis (raw byte ranges → display byte ranges).
        if kind == RowKind::Changed {
            for r in decor.map(|d| d.emph.as_slice()).unwrap_or(&[]) {
                let (a, b) =
                    (line[..r.start.min(line.len())].chars().count(), line[..r.end.min(line.len())].chars().count());
                let (a, b) = (byte(visual_col(line, a, TAB)), byte(visual_col(line, b, TAB)));
                if b > a {
                    hl.push((a..b, HighlightStyle { background_color: Some(emph_bg), ..Default::default() }));
                }
            }
        }
        // Selection within this row, as byte range of the display string.
        let sel = self.buffer.selection().and_then(|(s, e)| {
            if ix < s.row || ix > e.row {
                return None;
            }
            let from = if ix == s.row { visual_col(line, s.col, TAB) } else { 0 };
            let to = if ix == e.row { visual_col(line, e.col, TAB) } else { display.chars().count() + 1 };
            Some((from, to))
        });
        let mut text = display.clone();
        let mut sel_hl = Vec::new();
        if let Some((from, to)) = sel {
            // Selected line breaks show as one extra highlighted space.
            if to > display.chars().count() {
                text.push(' ');
            }
            let (a, b) = (byte(from), if to > display.chars().count() { text.len() } else { byte(to) });
            if b > a {
                sel_hl.push((
                    a..b,
                    HighlightStyle { background_color: Some(theme::frost().opacity(0.25)), ..Default::default() },
                ));
            }
        }
        let hl: Vec<_> = gpui::combine_highlights(hl, sel_hl).collect();
        let caret_x = gutter_w + visual_col(line, caret.col, TAB) as f32 * self.char_w;
        let row_bg = match kind {
            RowKind::Changed => Some(tint),
            _ if is_caret_row && focused => Some(theme::abyss()),
            _ => None,
        };
        div()
            .id(view)
            .h(theme::ROW_CODE)
            .min_w(px(self.min_row_w))
            .min_w_full()
            .relative()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .when_some(row_bg, |d, bg| d.bg(bg))
            .child(
                div()
                    .w(px(gutter_w))
                    .h_full()
                    .flex_none()
                    .pr(px(12.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .bg(if kind == RowKind::Changed { tint } else { theme::abyss() })
                    .border_r_1()
                    .border_color(theme::line())
                    .text_color(if is_caret_row && focused { theme::body() } else { theme::mute() })
                    .child((ix + 1).to_string()),
            )
            .child(
                div()
                    .pl(px(8.))
                    .pr(px(40.))
                    .text_color(theme::bone())
                    .child(StyledText::new(SharedString::from(text)).with_highlights(hl)),
            )
            .when(is_caret_row && focused, |d| {
                d.child(div().absolute().top(px(2.)).bottom(px(2.)).left(px(caret_x + 8.)).w(px(2.)).bg(theme::frost()))
            })
            .into_any_element()
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fid = window.text_system().resolve_font(&gpui::font(self.font.clone()));
        if let Ok(size) = window.text_system().advance(fid, theme::TEXT_CODE, 'm') {
            self.char_w = size.width.into();
        }
        let count = self.view_rows();
        let widest = self.line_view_row(self.buffer.widest_row());
        let empty = self.buffer.is_empty();
        div()
            .id("editor")
            .key_context("KerfEditor")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(theme::void())
            .font_family(self.font.clone())
            .text_size(theme::TEXT_CODE)
            .cursor_text()
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.dragging = false))
            .on_action(cx.listener(|this, _: &Left, _, cx| this.mv(Move::Left, false, cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.mv(Move::Right, false, cx)))
            .on_action(cx.listener(|this, _: &Up, _, cx| this.mv(Move::Up, false, cx)))
            .on_action(cx.listener(|this, _: &Down, _, cx| this.mv(Move::Down, false, cx)))
            .on_action(cx.listener(|this, _: &WordLeft, _, cx| this.mv(Move::WordLeft, false, cx)))
            .on_action(cx.listener(|this, _: &WordRight, _, cx| this.mv(Move::WordRight, false, cx)))
            .on_action(cx.listener(|this, _: &LineStart, _, cx| this.mv(Move::LineStart, false, cx)))
            .on_action(cx.listener(|this, _: &LineEnd, _, cx| this.mv(Move::LineEnd, false, cx)))
            .on_action(cx.listener(|this, _: &DocStart, _, cx| this.mv(Move::DocStart, false, cx)))
            .on_action(cx.listener(|this, _: &DocEnd, _, cx| this.mv(Move::DocEnd, false, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                let n = this.page_rows();
                this.mv(Move::PageUp(n), false, cx)
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                let n = this.page_rows();
                this.mv(Move::PageDown(n), false, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| this.mv(Move::Left, true, cx)))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| this.mv(Move::Right, true, cx)))
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| this.mv(Move::Up, true, cx)))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| this.mv(Move::Down, true, cx)))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| this.mv(Move::WordLeft, true, cx)))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| this.mv(Move::WordRight, true, cx)))
            .on_action(cx.listener(|this, _: &SelectLineStart, _, cx| this.mv(Move::LineStart, true, cx)))
            .on_action(cx.listener(|this, _: &SelectLineEnd, _, cx| this.mv(Move::LineEnd, true, cx)))
            .on_action(cx.listener(|this, _: &SelectDocStart, _, cx| this.mv(Move::DocStart, true, cx)))
            .on_action(cx.listener(|this, _: &SelectDocEnd, _, cx| this.mv(Move::DocEnd, true, cx)))
            .on_action(cx.listener(|this, _: &Backspace, _, cx| {
                this.buffer.backspace(false);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &BackspaceWord, _, cx| {
                this.buffer.backspace(true);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &Delete, _, cx| {
                this.buffer.delete_forward(false);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &DeleteWord, _, cx| {
                this.buffer.delete_forward(true);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &Newline, _, cx| {
                this.buffer.newline();
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &InsertTab, _, cx| {
                // Tab on a multi-line selection indents it; otherwise inserts spaces.
                if this.buffer.multi_line_selection() {
                    this.buffer.indent(TAB);
                } else {
                    this.buffer.insert(&" ".repeat(TAB));
                }
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &DeleteToLineStart, _, cx| {
                this.buffer.delete_to_line_start();
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &DeleteToLineEnd, _, cx| {
                this.buffer.delete_to_line_end();
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &ClearAll, _, cx| {
                this.buffer.clear_all();
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &DeleteLine, _, cx| {
                this.buffer.delete_lines();
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &SelectLine, _, cx| {
                this.buffer.select_line();
                this.after_move(cx)
            }))
            .on_action(cx.listener(|this, _: &MoveLineUp, _, cx| {
                this.buffer.move_lines(false);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &MoveLineDown, _, cx| {
                this.buffer.move_lines(true);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &DuplicateLineUp, _, cx| {
                this.buffer.duplicate_lines(false);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &DuplicateLineDown, _, cx| {
                this.buffer.duplicate_lines(true);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &Indent, _, cx| {
                this.buffer.indent(TAB);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &Outdent, _, cx| {
                this.buffer.outdent(TAB);
                this.after_edit(cx)
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.buffer.select_all();
                cx.notify()
            }))
            .on_action(cx.listener(|this, _: &Copy, _, cx| {
                if let Some(t) = this.buffer.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(t));
                }
            }))
            .on_action(cx.listener(|this, _: &Cut, _, cx| {
                if let Some(t) = this.buffer.cut() {
                    cx.write_to_clipboard(ClipboardItem::new_string(t));
                    this.after_edit(cx)
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, _, cx| {
                if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    this.paste_text(&t, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Undo, _, cx| {
                if this.buffer.undo() {
                    this.after_edit(cx)
                }
            }))
            .on_action(cx.listener(|this, _: &Redo, _, cx| {
                if this.buffer.redo() {
                    this.after_edit(cx)
                }
            }))
            .on_action(cx.listener(|_, _: &Submit, _, cx| cx.emit(EditorEvent::Submit)))
            .on_action(cx.listener(|_, _: &Blur, _, cx| cx.emit(EditorEvent::Blur)))
            // Records this editor's bounds each frame for hit-testing.
            .child({
                let cell = self.bounds.clone();
                gpui::canvas(move |b, _, _| cell.set(b), |_, _, _, _| {}).absolute().size_full()
            })
            .child(
                uniform_list(
                    "editor-rows",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, window, _| {
                        let focused = this.focus.is_focused(window);
                        range.map(|ix| this.render_row(ix, focused)).collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.scroll.clone())
                .size_full()
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(Some(widest)),
            )
            .when(empty, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(0.))
                        .left(px(self.gutter_w() + 4.))
                        .h(theme::ROW_CODE)
                        .flex()
                        .items_center()
                        .text_color(theme::faint())
                        .child(self.placeholder.clone()),
                )
            })
    }
}
