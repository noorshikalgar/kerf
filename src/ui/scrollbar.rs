//! Scroll columns for the virtualized lists: a draggable scrollbar (sidebar list, diff body)
//! and, for the diff, a minimap column of change markers beside it.
//! The math is pure so it can be tested; `Kerf` owns the drag state.

use super::app::Kerf;
use crate::theme;
use gpui::{div, point, prelude::*, px, AnyElement, Context, MouseButton, MouseDownEvent, UniformListScrollHandle};

/// Smallest thumb, so it stays grabbable on huge lists.
pub const MIN_THUMB: f32 = 24.;
pub const WIDTH: f32 = 12.;
pub const MAP_WIDTH: f32 = 18.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    List,
    Diff,
}

#[derive(Debug, Clone, Copy)]
pub struct Drag {
    pub bar: Bar,
    /// Distance from the thumb top to the pointer when the drag started.
    pub grab: f32,
    /// Dragging on the minimap: the pointer position centres the viewport.
    pub map: bool,
}

/// Scroll geometry in pixels. `offset` is positive (distance scrolled down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub track_top: f32,
    pub viewport: f32,
    pub content: f32,
    pub offset: f32,
}

impl Metrics {
    pub fn scrollable(&self) -> bool {
        self.content > self.viewport + 0.5 && self.viewport > 0.
    }
    pub fn max_offset(&self) -> f32 {
        (self.content - self.viewport).max(0.)
    }
    pub fn thumb_h(&self) -> f32 {
        if !self.scrollable() {
            return self.viewport;
        }
        (self.viewport * self.viewport / self.content).clamp(MIN_THUMB.min(self.viewport), self.viewport)
    }
    /// Thumb top relative to the track.
    pub fn thumb_top(&self) -> f32 {
        let travel = self.viewport - self.thumb_h();
        if travel <= 0. {
            return 0.;
        }
        (self.offset / self.max_offset()).clamp(0., 1.) * travel
    }
    /// Scroll offset that puts the thumb top at `thumb_top` (track-relative).
    pub fn offset_for(&self, thumb_top: f32) -> f32 {
        let travel = self.viewport - self.thumb_h();
        if travel <= 0. {
            return 0.;
        }
        (thumb_top / travel).clamp(0., 1.) * self.max_offset()
    }
    /// Minimap: offset that centres the viewport on track position `y` (track-relative).
    pub fn offset_for_map(&self, y: f32) -> f32 {
        if !self.scrollable() {
            return 0.;
        }
        let frac = (y / self.viewport).clamp(0., 1.);
        (frac * self.content - self.viewport / 2.).clamp(0., self.max_offset())
    }
    /// Minimap viewport frame (top, height), track-relative; content is scaled to fit the track.
    pub fn map_frame(&self) -> (f32, f32) {
        if !self.scrollable() {
            return (0., self.viewport);
        }
        let scale = self.viewport / self.content;
        ((self.offset * scale), (self.viewport * scale).max(6.))
    }
}

pub fn metrics(handle: &UniformListScrollHandle, rows: usize, row_h: f32) -> Metrics {
    let state = handle.0.borrow();
    let b = state.base_handle.bounds();
    // Note: despite its name, gpui's `last_item_size.item` is the list *viewport* size and
    // `contents` the full content size, both measured at the last layout. Prefer them.
    let (viewport, content) = match state.last_item_size {
        Some(s) if f32::from(s.item.height) > 0. => (f32::from(s.item.height), f32::from(s.contents.height)),
        _ => (f32::from(b.size.height), rows as f32 * row_h),
    };
    Metrics {
        track_top: b.origin.y.into(),
        viewport,
        content,
        offset: (-f32::from(state.base_handle.offset().y)).max(0.),
    }
}

impl Kerf {
    fn bar_handle(&self, bar: Bar) -> (&UniformListScrollHandle, usize, f32) {
        match bar {
            Bar::List => (&self.list_scroll, self.rows.len(), f32::from(theme::ROW_LIST)),
            Bar::Diff => match self.live_scroll() {
                // Plain diff tab: the two editors' shared scroll.
                Some((h, rows)) => (h, rows, f32::from(theme::ROW_CODE)),
                None => (&self.diff_scroll, self.visual_len(), f32::from(theme::ROW_CODE)),
            },
        }
    }

    pub fn bar_metrics(&self, bar: Bar) -> Metrics {
        let (h, rows, row_h) = self.bar_handle(bar);
        metrics(h, rows, row_h)
    }

    /// Mouse down on a track: grab the thumb where clicked, or centre it on the pointer.
    pub fn begin_scroll_drag(&mut self, bar: Bar, y: f32, cx: &mut Context<Self>) {
        let m = self.bar_metrics(bar);
        if !m.scrollable() {
            return;
        }
        let rel = y - m.track_top;
        let top = m.thumb_top();
        let grab = if rel >= top && rel <= top + m.thumb_h() { rel - top } else { m.thumb_h() / 2. };
        self.scroll_drag = Some(Drag { bar, grab, map: false });
        self.drag_scroll_to(y, cx);
    }

    /// Pointer moved while dragging: follow it.
    pub fn drag_scroll_to(&mut self, y: f32, cx: &mut Context<Self>) {
        let Some(drag) = self.scroll_drag else { return };
        let m = self.bar_metrics(drag.bar);
        let offset =
            if drag.map { m.offset_for_map(y - m.track_top) } else { m.offset_for(y - m.track_top - drag.grab) };
        let (h, _, _) = self.bar_handle(drag.bar);
        let base = &h.0.borrow().base_handle;
        let x = base.offset().x;
        base.set_offset(point(x, px(-offset)));
        cx.notify();
    }

    /// Scrollbar column: track + draggable thumb. Sits beside the list, never over it.
    pub fn render_scrollbar(&self, bar: Bar, cx: &mut Context<Self>) -> AnyElement {
        let m = self.bar_metrics(bar);
        let dragging = self.scroll_drag.is_some_and(|d| d.bar == bar && !d.map);
        let show_thumb = m.scrollable();
        div()
            .id(match bar {
                Bar::List => "list-scrollbar",
                Bar::Diff => "diff-scrollbar",
            })
            .relative()
            .flex_none()
            .h_full()
            .w(px(WIDTH))
            .bg(theme::abyss())
            // The sidebar list's scrollbar sits against the sidebar's own border: no second edge.
            .when(bar == Bar::Diff, |d| d.border_l_1().border_color(theme::line()))
            .when(show_thumb, |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.begin_scroll_drag(bar, ev.position.y.into(), cx);
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(2.))
                        .right(px(2.))
                        .top(px(m.thumb_top()))
                        .h(px(m.thumb_h()))
                        .rounded(px(4.))
                        .bg(if dragging { theme::thumb_active() } else { theme::thumb() })
                        .hover(|s| s.bg(theme::thumb_hover())),
                )
            })
            .into_any_element()
    }

    /// Minimap column for the diff: change markers + a frame showing what's on screen.
    /// Click or drag to centre the view there.
    pub fn render_map(&self, markers: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let m = self.bar_metrics(Bar::Diff);
        let (top, h) = m.map_frame();
        let active = self.scroll_drag.is_some_and(|d| d.map);
        div()
            .id("diff-map")
            .relative()
            .flex_none()
            .h_full()
            .w(px(MAP_WIDTH))
            .bg(theme::void())
            .border_l_1()
            .border_color(theme::line())
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if this.bar_metrics(Bar::Diff).scrollable() {
                        this.scroll_drag = Some(Drag { bar: Bar::Diff, grab: 0., map: true });
                        this.drag_scroll_to(ev.position.y.into(), cx);
                    }
                }),
            )
            .children(markers)
            .when(m.scrollable(), |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(top))
                        .h(px(h))
                        .bg(theme::wash(if active { 0.12 } else { 0.06 }))
                        .border_y_1()
                        .border_color(if active { theme::frost() } else { theme::mute() }),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(offset: f32) -> Metrics {
        Metrics { track_top: 100., viewport: 400., content: 4000., offset }
    }

    #[test]
    fn thumb_size_is_proportional_with_minimum() {
        assert_eq!(m(0.).thumb_h(), 40.);
        let huge = Metrics { content: 4_000_000., ..m(0.) };
        assert_eq!(huge.thumb_h(), MIN_THUMB);
    }

    #[test]
    fn thumb_position_tracks_offset() {
        assert_eq!(m(0.).thumb_top(), 0.);
        assert_eq!(m(3600.).thumb_top(), 360.); // bottom: travel = 400 - 40
        assert_eq!(m(1800.).thumb_top(), 180.);
    }

    #[test]
    fn offset_for_inverts_thumb_top() {
        let mm = m(0.);
        for top in [0., 90., 180., 360.] {
            let off = mm.offset_for(top);
            assert!((Metrics { offset: off, ..mm }.thumb_top() - top).abs() < 0.01);
        }
        assert_eq!(mm.offset_for(-50.), 0.);
        assert_eq!(mm.offset_for(9999.), 3600.);
    }

    #[test]
    fn map_click_centres_viewport() {
        let mm = m(0.);
        // click middle of track → content middle centred
        assert_eq!(mm.offset_for_map(200.), 1800.);
        assert_eq!(mm.offset_for_map(0.), 0.);
        assert_eq!(mm.offset_for_map(400.), 3600.);
        let (top, h) = m(1800.).map_frame();
        assert_eq!((top, h), (180., 40.));
    }

    #[test]
    fn not_scrollable_when_content_fits() {
        let fit = Metrics { content: 300., ..m(0.) };
        assert!(!fit.scrollable());
        assert_eq!(fit.offset_for(100.), 0.);
    }
}
