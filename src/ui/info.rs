//! "PR Merge View vs Compare View" explainer popup (F1 / `?` button).

use super::app::Kerf;
use super::widgets::micro;
use crate::git::RangeMode;
use crate::theme;
use gpui::{div, prelude::*, px, relative, AnyElement, Context, Div, FontWeight, Hsla, Window};

impl Kerf {
    pub fn render_info(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.info_open {
            return None;
        }
        let base = self.base.as_deref().map(|b| self.display_ref(b)).unwrap_or_else(|| "main".into());
        let cmp = self.compare.as_deref().map(|c| self.display_ref(c)).unwrap_or_else(|| "feature".into());
        let pad = |s: &str, n: usize| {
            let mut t: String = s.chars().take(n).collect();
            while t.chars().count() < n {
                t.push(' ');
            }
            t
        };
        let diagram = [
            format!("              B1 ── B2     ← {}", pad(&base, 18)),
            "             ╱".to_string(),
            "  A ── M ── ●".to_string(),
            "             ╲".to_string(),
            format!("              C1 ── C2     ← {}", pad(&cmp, 18)),
            String::new(),
            "  ● merge base — where the branches split".to_string(),
        ];

        let mode = self.mode;
        Some(
            div()
                .id("info-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(gpui::hsla(0., 0., 0., 0.6))
                .flex()
                .items_center()
                .justify_center()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.info_open = false;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("info")
                        .w(px(720.))
                        .max_w(relative(0.9))
                        .max_h(relative(0.85))
                        // Vertical scroll only: everything inside wraps to the popup width.
                        .overflow_y_scroll()
                        .overflow_x_hidden()
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .p(px(24.))
                        .bg(theme::crypt())
                        .border_1()
                        .border_color(theme::line_hi())
                        .rounded(theme::RADIUS)
                        .shadow_lg()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(theme::TEXT_DISPLAY)
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme::bone())
                                        .child("PR Merge View vs Compare View"),
                                )
                                .child(
                                    div()
                                        .id("info-close")
                                        .px(px(8.))
                                        .py(px(2.))
                                        .rounded(theme::RADIUS)
                                        .text_size(theme::TEXT_CONTROL)
                                        .text_color(theme::mute())
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::ash()).text_color(theme::bone()))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.info_open = false;
                                            cx.notify();
                                        }))
                                        .child("Esc"),
                                ),
                        )
                        .child(div().text_size(theme::TEXT_LIST).text_color(theme::body()).child(format!(
                            "Both compare {base} (base) with {cmp} (compare). They differ in the starting point."
                        )))
                        .child(
                            div()
                                .p(px(12.))
                                .overflow_x_hidden()
                                .bg(theme::void())
                                .border_1()
                                .border_color(theme::line())
                                .rounded(theme::RADIUS)
                                .text_size(theme::TEXT_CODE)
                                .text_color(theme::body())
                                .flex()
                                .flex_col()
                                .children(diagram.into_iter().map(|l| div().h(px(20.)).whitespace_nowrap().child(l))),
                        )
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .gap(px(12.))
                                .child(mode_card(
                                    RangeMode::PrMerge,
                                    mode == RangeMode::PrMerge,
                                    "What would this PR merge?",
                                    "From ● to C2",
                                    &[
                                        "Shows only C1 + C2 — the work done on compare.",
                                        "Ignores B1 + B2 (new commits on base).",
                                        "Same as GitHub’s “Files changed” tab.",
                                    ],
                                    "Reviewing a feature branch, self-review before opening a PR.",
                                    theme::add_fg(),
                                ))
                                .child(mode_card(
                                    RangeMode::Compare,
                                    mode == RangeMode::Compare,
                                    "How do the two tips differ right now?",
                                    "From B2 to C2",
                                    &[
                                        "Shows C1 + C2 and B1 + B2 reversed.",
                                        "Work only on base looks “removed”.",
                                        "Exactly what changes if compare replaced base.",
                                    ],
                                    "Releases, tags, two arbitrary commits, deploy diffs.",
                                    theme::frost(),
                                )),
                        )
                        .child(micro("Example"))
                        .child(
                            div()
                                .text_size(theme::TEXT_LIST)
                                .text_color(theme::body())
                                .child(format!("You add login.rs on {cmp}. A teammate adds billing.rs on {base}.")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .border_1()
                                .border_color(theme::line())
                                .rounded(theme::RADIUS)
                                .child(example_row("PR Merge View", &[("+ login.rs", theme::add_fg())]))
                                .child(example_row(
                                    "Compare View",
                                    &[("+ login.rs", theme::add_fg()), ("− billing.rs", theme::del_fg())],
                                )),
                        )
                        .child(
                            div()
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::mute())
                                .child("Switch with ⌘⇧M. Branches with no shared history always use Compare View."),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn mode_card(
    mode: RangeMode,
    active: bool,
    question: &str,
    range: &str,
    points: &[&str],
    use_for: &str,
    accent: Hsla,
) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(6.))
        .p(px(12.))
        .rounded(theme::RADIUS)
        .border_1()
        .border_color(if active { accent } else { theme::line_hi() })
        .bg(if active { theme::slate() } else { theme::abyss() })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(theme::TEXT_LIST)
                        .font_weight(FontWeight::BOLD)
                        .text_color(accent)
                        .child(mode.label()),
                )
                .when(active, |d| {
                    d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::frost()).child("Current"))
                }),
        )
        .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(mode.git()))
        .child(div().text_size(theme::TEXT_LIST).text_color(theme::bone()).child(question.to_string()))
        .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::body()).child(range.to_string()))
        .children(points.iter().map(|p| {
            div()
                .flex()
                .gap(px(6.))
                .text_size(theme::TEXT_CONTROL)
                .text_color(theme::body())
                .child(div().flex_none().text_color(theme::mute()).child("•"))
                .child(div().flex_1().min_w_0().child(p.to_string()))
        }))
        .child(
            div()
                .mt(px(4.))
                .text_size(theme::TEXT_CONTROL)
                .text_color(theme::mute())
                .child(format!("Use for: {use_for}")),
        )
}

fn example_row(label: &str, items: &[(&str, Hsla)]) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(16.))
        .px(px(12.))
        .h(px(30.))
        .border_b_1()
        .border_color(theme::line())
        .child(div().w(px(140.)).text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(label.to_string()))
        .children(
            items
                .iter()
                .map(|(t, c)| div().flex_none().text_size(theme::TEXT_LIST).text_color(*c).child(t.to_string())),
        )
}
