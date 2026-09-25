//! Fuzzy ref picker popover (Base / Compare).

use super::app::{Kerf, Which};
use super::widgets::{age, micro};
use crate::git::{short_sha, RefKind};
use crate::theme;
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, HighlightStyle, StyledText, Window};

const MAX_SHOWN: usize = 200;

impl Kerf {
    pub fn render_picker(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.picker.as_ref()?;
        let matches = self.picker_matches();
        let selected = p.selected.min(matches.len().saturating_sub(1));
        let title = match p.which {
            Which::Base => "Base — compare against",
            Which::Compare => "Compare — changes to see",
        };
        let current = match p.which {
            Which::Base => self.base.clone(),
            Which::Compare => self.compare.clone(),
        };
        let top = f32::from(theme::TITLEBAR_H) + 64. + if p.which == Which::Compare { 52. } else { 0. };
        let left = 12.;
        let width = (self.sidebar_w + 180.).min(620.);

        let mut rows: Vec<AnyElement> = Vec::new();
        let mut last_kind: Option<RefKind> = None;
        let grouped = p.query.is_empty();
        for (n, (ref_idx, positions)) in matches.iter().take(MAX_SHOWN).enumerate() {
            let r = &self.refs[*ref_idx];
            if grouped && last_kind != Some(r.kind) {
                last_kind = Some(r.kind);
                rows.push(
                    micro(match r.kind {
                        RefKind::Local => "Local branches",
                        RefKind::Remote => "Remote branches",
                        RefKind::Tag => "Tags",
                    })
                    .px(px(10.))
                    .pt(px(8.))
                    .pb(px(2.))
                    .into_any_element(),
                );
            }
            let is_sel = n == selected;
            let is_current = current.as_deref() == Some(r.name.as_str());
            let hl: Vec<_> = positions
                .iter()
                .filter_map(|&ci| {
                    let (b, ch) = r.name.char_indices().nth(ci)?;
                    Some((b..b + ch.len_utf8(), HighlightStyle { color: Some(theme::frost()), font_weight: Some(FontWeight::BOLD), ..Default::default() }))
                })
                .collect();
            let idx = *ref_idx;
            rows.push(
                div()
                    .id(("ref", idx))
                    .h(theme::ROW_LIST)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(10.))
                    .text_size(theme::TEXT_LIST)
                    .border_l_2()
                    .border_color(if is_sel { theme::frost() } else { gpui::transparent_black() })
                    .when(is_sel, |d| d.bg(theme::slate()))
                    .when(!is_sel, |d| d.hover(|s| s.bg(theme::ash())))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.pick(idx, cx)))
                    .child(
                        div()
                            .w(px(10.))
                            .flex_none()
                            .text_color(theme::add_fg())
                            .child(if r.is_head { "●" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(if is_sel { theme::bone() } else { theme::body() })
                            .child(StyledText::new(r.name.clone()).with_highlights(hl)),
                    )
                    .when(is_current, |d| d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::frost()).child("CURRENT")))
                    .child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(short_sha(r.target)))
                    .child(div().w(px(32.)).flex_none().flex().justify_end().text_size(theme::TEXT_CONTROL).text_color(theme::faint()).child(age(r.time)))
                    .into_any_element(),
            );
        }
        if matches.is_empty() {
            rows.push(
                div()
                    .px(px(10.))
                    .py(px(8.))
                    .text_size(theme::TEXT_LIST)
                    .text_color(theme::mute())
                    .child("No refs match")
                    .into_any_element(),
            );
        }
        let overflow = matches.len().saturating_sub(MAX_SHOWN);

        Some(
            div()
                .id("picker-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.picker = None;
                    this.input = super::app::Input::None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("picker")
                        .absolute()
                        .top(px(top))
                        .left(px(left))
                        .w(px(width))
                        .max_h(px(460.))
                        .flex()
                        .flex_col()
                        .bg(theme::crypt())
                        .border_1()
                        .border_color(theme::line_hi())
                        .rounded(theme::RADIUS)
                        .shadow_lg()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex_none()
                                .px(px(10.))
                                .pt(px(8.))
                                .pb(px(6.))
                                .border_b_1()
                                .border_color(theme::line())
                                .child(micro(title).mb(px(4.)))
                                .child(
                                    div()
                                        .h(theme::CONTROL_H)
                                        .flex()
                                        .items_center()
                                        .text_size(theme::TEXT_LIST)
                                        .child(div().text_color(theme::frost()).mr(px(6.)).child("›"))
                                        .when(p.query.is_empty(), |d| d.child(div().text_color(theme::mute()).child("type to filter branches and tags…")))
                                        .when(!p.query.is_empty(), |d| d.child(div().text_color(theme::bone()).child(p.query.clone())))
                                        .child(div().w(px(1.)).h(px(14.)).bg(theme::frost())),
                                ),
                        )
                        .child(div().id("picker-list").flex_1().min_h_0().overflow_y_scroll().py(px(4.)).children(rows))
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .gap(px(12.))
                                .px(px(10.))
                                .py(px(4.))
                                .border_t_1()
                                .border_color(theme::line())
                                .text_size(theme::TEXT_MICRO)
                                .text_color(theme::faint())
                                .child("↑↓ move")
                                .child("↵ pick")
                                .child("esc close")
                                .when(overflow > 0, |d| d.child(div().flex_1()).child(format!("+{overflow} more — keep typing"))),
                        ),
                )
                .into_any_element(),
        )
    }
}
