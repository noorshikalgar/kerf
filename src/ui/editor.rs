//! Minimal code editor view for plain diffs: virtualized rows, gutter, caret, selection,
//! keyboard editing, clipboard, undo. Editing rules live in `crate::buffer`.

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
    ]
);

pub fn init(cx: &mut App) {
    let c = Some("KerfEditor");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("alt-backspace", BackspaceWord, c),
        KeyBinding::new("delete", Delete, c),
        KeyBinding::new("alt-delete", DeleteWord, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("up", Up, c),
        KeyBinding::new("down", Down, c),
        KeyBinding::new("alt-left", WordLeft, c),
        KeyBinding::new("alt-right", WordRight, c),
        KeyBinding::new("cmd-left", LineStart, c),
        KeyBinding::new("cmd-right", LineEnd, c),
        KeyBinding::new("home", LineStart, c),
        KeyBinding::new("end", LineEnd, c),
        KeyBinding::new("cmd-up", DocStart, c),
        KeyBinding::new("cmd-down", DocEnd, c),
        KeyBinding::new("pageup", PageUp, c),
        KeyBinding::new("pagedown", PageDown, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("alt-shift-left", SelectWordLeft, c),
        KeyBinding::new("alt-shift-right", SelectWordRight, c),
        KeyBinding::new("cmd-shift-left", SelectLineStart, c),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, c),
        KeyBinding::new("shift-home", SelectLineStart, c),
        KeyBinding::new("shift-end", SelectLineEnd, c),
        KeyBinding::new("cmd-shift-up", SelectDocStart, c),
        KeyBinding::new("cmd-shift-down", SelectDocEnd, c),
        KeyBinding::new("enter", Newline, c),
        KeyBinding::new("tab", InsertTab, c),
        KeyBinding::new("cmd-a", SelectAll, c),
        KeyBinding::new("cmd-c", Copy, c),
        KeyBinding::new("cmd-x", Cut, c),
        KeyBinding::new("cmd-v", Paste, c),
        KeyBinding::new("cmd-z", Undo, c),
        KeyBinding::new("cmd-shift-z", Redo, c),
        KeyBinding::new("cmd-enter", Submit, c),
        KeyBinding::new("escape", Blur, c),
    ]);
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
    char_w: f32,
    dragging: bool,
}

impl EventEmitter<EditorEvent> for Editor {}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Editor {
    pub fn new(text: &str, font: SharedString, placeholder: &str, cx: &mut Context<Self>) -> Self {
        Self {
            buffer: Buffer::new(text),
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            font,
            placeholder: placeholder.to_string().into(),
            char_w: 8.4,
            dragging: false,
        }
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
        let digits = self.buffer.line_count().to_string().len().max(3);
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
        self.scroll.scroll_to_item(c.row, ScrollStrategy::Top);
        let state = self.scroll.0.borrow();
        let base = &state.base_handle;
        let view_w: f32 = base.bounds().size.width.into();
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
        let b = state.base_handle.bounds();
        let off = state.base_handle.offset();
        let y: f32 = (p.y - b.origin.y - off.y).into();
        let row = ((y / f32::from(theme::ROW_CODE)).floor().max(0.) as usize).min(self.buffer.line_count() - 1);
        let x: f32 = (p.x - b.origin.x - off.x).into();
        let vcol = (x - self.gutter_w()) / self.char_w;
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

    fn render_row(&self, ix: usize, focused: bool) -> impl IntoElement {
        let line = self.buffer.line(ix);
        let caret = self.buffer.caret();
        let is_caret_row = caret.row == ix;
        let display: String = line.replace('\t', &" ".repeat(TAB));
        // Selection within this row, as byte range of the display string.
        let sel = self.buffer.selection().and_then(|(s, e)| {
            if ix < s.row || ix > e.row {
                return None;
            }
            let from = if ix == s.row { visual_col(line, s.col, TAB) } else { 0 };
            let to = if ix == e.row { visual_col(line, e.col, TAB) } else { display.chars().count() + 1 };
            Some((from, to))
        });
        let byte = |vc: usize| display.char_indices().nth(vc).map(|(b, _)| b).unwrap_or(display.len());
        let mut hl = Vec::new();
        let mut text = display.clone();
        if let Some((from, to)) = sel {
            // Selected line breaks show as one extra highlighted space.
            if to > display.chars().count() {
                text.push(' ');
            }
            let (a, b) = (byte(from), if to > display.chars().count() { text.len() } else { byte(to) });
            if b > a {
                hl.push((a..b, HighlightStyle { background_color: Some(theme::frost().opacity(0.25)), ..Default::default() }));
            }
        }
        let gutter_w = self.gutter_w();
        let caret_x = gutter_w + visual_col(line, caret.col, TAB) as f32 * self.char_w;
        div()
            .id(ix)
            .h(theme::ROW_CODE)
            .min_w_full()
            .relative()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .when(is_caret_row && focused, |d| d.bg(theme::abyss()))
            .child(
                div()
                    .w(px(gutter_w))
                    .flex_none()
                    .pr(px(12.))
                    .flex()
                    .justify_end()
                    .text_color(if is_caret_row && focused { theme::body() } else { theme::mute() })
                    .child((ix + 1).to_string()),
            )
            .child(div().pr(px(40.)).text_color(theme::bone()).child(StyledText::new(SharedString::from(text)).with_highlights(hl)))
            .when(is_caret_row && focused, |d| {
                d.child(div().absolute().top(px(2.)).bottom(px(2.)).left(px(caret_x)).w(px(2.)).bg(theme::frost()))
            })
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fid = window.text_system().resolve_font(&gpui::font(self.font.clone()));
        if let Ok(size) = window.text_system().advance(fid, theme::TEXT_CODE, 'm') {
            self.char_w = size.width.into();
        }
        let count = self.buffer.line_count();
        let widest = self.buffer.widest_row();
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
                this.buffer.insert(&" ".repeat(TAB));
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
