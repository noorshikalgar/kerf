//! Theme switcher: status-bar menu + View menu actions. The choice is saved and applies to
//! every window at once.

use super::app::Kerf;
use super::widgets::micro;
use crate::theme::{self, palette_of, ThemeId};
use gpui::{div, prelude::*, px, rgb, AnyElement, Context, FontWeight, Hsla};

impl Kerf {
    pub fn set_theme(&mut self, id: ThemeId, cx: &mut Context<Self>) {
        theme::set_current(id);
        self.persisted.theme = Some(id.key().to_string());
        self.persisted.save();
        self.theme_menu_open = false;
        cx.refresh_windows();
        cx.notify();
    }

    pub fn render_theme_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.theme_menu_open {
            return None;
        }
        let current = theme::current();
        let nerd = self.nerd();
        let row = |id: ThemeId, cx: &mut Context<Self>| {
            let p = palette_of(id);
            let chip = |hex: u32| div().w(px(12.)).h(px(20.)).bg(Hsla::from(rgb(hex)));
            let active = id == current;
            div()
                .id(id.key())
                .mx(px(6.))
                .px(px(8.))
                .h(px(40.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(6.))
                .cursor_pointer()
                .when(active, |d| d.bg(theme::slate()))
                .when(!active, |d| d.hover(|s| s.bg(theme::ash())))
                .on_click(cx.listener(move |this, _, _, cx| this.set_theme(id, cx)))
                // Swatch: canvas, text, added, removed, accent.
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .rounded(px(4.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(theme::line_hi())
                        .child(chip(p.void))
                        .child(chip(p.bone))
                        .child(chip(p.add_fg))
                        .child(chip(p.del_fg))
                        .child(chip(p.frost)),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(theme::TEXT_LIST)
                        .font_weight(if active { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                        .text_color(theme::bone())
                        .child(id.name()),
                )
                .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(if id.is_dark() {
                    "Dark"
                } else {
                    "Light"
                }))
                .when(active, |d| {
                    d.child(div().w(px(16.)).text_color(theme::frost()).child(if nerd { "\u{eab2}" } else { "✓" }))
                })
                .when(!active, |d| d.child(div().w(px(16.))))
        };
        Some(
            div()
                .id("theme-menu-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.theme_menu_open = false;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("theme-menu")
                        .absolute()
                        .right(px(12.))
                        .bottom(theme::STATUS_H + px(6.))
                        .w(px(300.))
                        .py(px(6.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .bg(theme::crypt())
                        .border_1()
                        .border_color(theme::line_hi())
                        .rounded(px(10.))
                        .shadow_lg()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(micro("Theme").px(px(14.)).pt(px(4.)).pb(px(4.)))
                        .children(ThemeId::ALL.into_iter().map(|id| row(id, cx))),
                )
                .into_any_element(),
        )
    }
}
