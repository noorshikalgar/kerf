//! All keyboard shortcuts in one popup, opened from "Shortcuts" in the status bar.
//! Shortcuts are listed here instead of being sprinkled through the UI.

use super::app::Kerf;
use super::widgets::micro;
use crate::theme;
use gpui::{div, prelude::*, px, relative, AnyElement, Context, Div, FontWeight, Window};

/// (title, [(keys, action)])
type Group = (&'static str, &'static [(&'static str, &'static str)]);

const GROUPS: &[Group] = &[
    (
        "Repository & Range",
        &[
            ("⌘O", "Open repository"),
            ("⌘R", "Refresh branches and diff"),
            ("⌘1", "Pick base"),
            ("⌘2", "Pick compare"),
            ("⌘⇧S", "Swap base and compare"),
            ("⌘⇧M", "PR Merge View / Compare View"),
            ("F1  ?", "Explain the two views"),
        ],
    ),
    (
        "Sidebar",
        &[
            ("⌘⇧F", "Files tab"),
            ("⌘⇧C", "Commits tab"),
            ("⌘B", "Show / hide sidebar"),
            ("↑ ↓  k j", "Move selection (diff follows)"),
            ("← →  h l", "Collapse / expand"),
            ("/", "Filter files"),
            ("t", "Tree / list"),
            ("y", "Copy path or SHA"),
            ("b  c", "Commit → base / compare"),
        ],
    ),
    (
        "Diff",
        &[
            ("]  [", "Next / previous file"),
            ("n  p", "Next / previous hunk"),
            ("space  ⇧space", "Page down / up"),
            ("g  G", "Top / bottom"),
            ("s", "Unified / split"),
            ("w", "Ignore whitespace"),
            ("z", "Wrap long lines"),
            ("↵", "Load large file · keep tab open"),
        ],
    ),
    (
        "Tabs",
        &[
            ("⌘W", "Close tab"),
            ("⌘⇧N", "New window"),
            ("⌘⇧W", "Close window"),
            ("⌘⇧]  ⌃Tab", "Next tab"),
            ("⌘⇧[  ⌃⇧Tab", "Previous tab"),
            ("double-click", "Keep preview tab open"),
        ],
    ),
    (
        "Plain Diff",
        &[
            ("⌘N", "New diff — type or paste on both sides"),
            ("⌥⌘N", "Compare two files (editable)"),
            ("⌘V", "Paste (editor, or first empty side)"),
            ("⌘Z  ⌘⇧Z", "Undo / redo in editor"),
            ("⌘⌫  ⌘⌦", "Delete to line start / end"),
            ("⌘⇧⌫", "Clear all text (undoable)"),
            ("⌘⇧K", "Delete line"),
            ("⌘L", "Select line"),
            ("⌥↑ ⌥↓", "Move line up / down"),
            ("⇧⌥↑ ⇧⌥↓", "Duplicate line"),
            ("⌘] ⌘[  ⇥ ⇧⇥", "Indent / outdent"),
            ("⌥← ⌥→", "Move by word (⇧ selects)"),
            ("Esc", "Leave the editor"),
        ],
    ),
];

impl Kerf {
    pub fn render_shortcuts(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.shortcuts_open {
            return None;
        }
        let close = cx.listener(|this, _, _, cx| {
            this.shortcuts_open = false;
            cx.notify();
        });
        let (left, right): (Vec<_>, Vec<_>) = GROUPS.iter().enumerate().partition(|(i, _)| i % 2 == 0);
        let column = |groups: Vec<(usize, &Group)>| {
            div().flex_1().min_w_0().flex().flex_col().gap(px(16.)).children(groups.into_iter().map(|(_, (title, keys))| group(title, keys)))
        };
        Some(
            div()
                .id("shortcuts-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(gpui::hsla(0., 0., 0., 0.6))
                .flex()
                .items_center()
                .justify_center()
                .on_click(close)
                .child(
                    div()
                        .id("shortcuts")
                        .w(px(760.))
                        .max_w(relative(0.9))
                        .max_h(relative(0.85))
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
                                        .child("Shortcuts"),
                                )
                                .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child("Esc to close")),
                        )
                        .child(div().flex().gap(px(32.)).child(column(left)).child(column(right))),
                )
                .into_any_element(),
        )
    }
}

fn group(title: &str, keys: &[(&str, &str)]) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(micro(title.to_string()).mb(px(4.)))
        .children(keys.iter().map(|(k, what)| {
            div()
                .h(px(26.))
                .flex()
                .items_center()
                .gap(px(12.))
                .border_b_1()
                .border_color(theme::line())
                .child(div().w(px(130.)).flex_none().text_size(theme::TEXT_LIST).text_color(theme::frost()).child(k.to_string()))
                .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().text_size(theme::TEXT_LIST).text_color(theme::body()).child(what.to_string()))
        }))
}
