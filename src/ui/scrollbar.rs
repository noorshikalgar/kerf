//! Draggable scrollbars for the virtualized lists (sidebar list, diff body).
//! The math is pure so it can be tested; `Kerf` owns the drag state.

use super::app::Kerf;
use crate::theme;
use gpui::{div, point, prelude::*, px, AnyElement, Context, MouseButton, MouseDownEvent, UniformListScrollHandle};

/// Smallest thumb, so it stays grabbable on huge lists.
pub const MIN_THUMB: f32 = 24.;
pub const WIDTH: f32 = 10.;

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
}

pub fn metrics(handle: &UniformListScrollHandle, rows: usize, row_h: f32) -> Metrics {
    let state = handle.0.borrow();
    let b = state.base_handle.bounds();
    let item_h = state.last_item_size.map(|s| f32::from(s.item.height)).filter(|h| *h > 0.).unwrap_or(row_h);
    Metrics {
        track_top: b.origin.y.into(),
        viewport: b.size.height.into(),
        content: rows as f32 * item_h,
        offset: (-f32::from(state.base_handle.offset().y)).max(0.),
    }
}

impl Kerf {
    fn bar_handle(&self, bar: Bar) -> (&UniformListScrollHandle, usize, f32) {
        match bar {
            Bar::List => (&self.list_scroll, self.rows.len(), f32::from(theme::ROW_LIST)),
            Bar::Diff => (
                &self.diff_scroll,
                self.loaded().map(|l| l.rows.rows.len()).unwrap_or(0),
                f32::from(theme::ROW_CODE),
            ),
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
        self.scroll_drag = Some(Drag { bar, grab });
        self.drag_scroll_to(y, cx);
    }

    /// Pointer moved while dragging: follow it.
    pub fn drag_scroll_to(&mut self, y: f32, cx: &mut Context<Self>) {
        let Some(drag) = self.scroll_drag else { return };
        let m = self.bar_metrics(drag.bar);
        let offset = m.offset_for(y - m.track_top - drag.grab);
        let (h, _, _) = self.bar_handle(drag.bar);
        let base = &h.0.borrow().base_handle;
        let x = base.offset().x;
        base.set_offset(point(x, px(-offset)));
        cx.notify();
    }

    /// Track + thumb. `extra` is painted under the thumb (diff hunk markers).
    pub fn render_scrollbar(&self, bar: Bar, extra: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let m = self.bar_metrics(bar);
        let dragging = self.scroll_drag.is_some_and(|d| d.bar == bar);
        let show_thumb = m.scrollable();
        div()
            .id(match bar {
                Bar::List => "list-scrollbar",
                Bar::Diff => "diff-scrollbar",
            })
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(WIDTH))
            .when(bar == Bar::Diff, |d| d.bg(theme::abyss()).border_l_1().border_color(theme::line()))
            .when(show_thumb || bar == Bar::Diff, |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.begin_scroll_drag(bar, ev.position.y.into(), cx);
                    }),
                )
            })
            .children(extra)
            .when(show_thumb, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(2.))
                        .right(px(2.))
                        .top(px(m.thumb_top()))
                        .h(px(m.thumb_h()))
                        .rounded(px(3.))
                        .bg(if dragging { theme::mute() } else { theme::thumb() })
                        .when(bar == Bar::Diff, |d| d.bg(if dragging { gpui::hsla(0., 0., 1., 0.22) } else { gpui::hsla(0., 0., 1., 0.10) }))
                        .hover(|s| s.bg(theme::line_hi())),
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
    fn not_scrollable_when_content_fits() {
        let fit = Metrics { content: 300., ..m(0.) };
        assert!(!fit.scrollable());
        assert_eq!(fit.offset_for(100.), 0.);
    }
}
