//! Start page (Zed / VS Code style): shown when no repository is open and no tab is open.
//! The sidebar only exists in git mode, so this page has the whole window.

use super::app::Kerf;
use super::scratch::tilde;
use super::widgets::micro;
use crate::theme;
use gpui::{div, prelude::*, px, AnyElement, Context, Div, FontWeight, SharedString, Stateful};

const MAX_RECENTS: usize = 8;

fn icon(nerd: bool, glyph: &'static str) -> Div {
    div().w(px(20.)).flex_none().text_size(theme::TEXT_CODE).text_color(theme::mute()).child(if nerd {
        glyph
    } else {
        "›"
    })
}

/// A primary start action as a card: icon, label, one-line explanation.
fn card(id: &'static str, nerd: bool, glyph: &'static str, label: &'static str, sub: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(200.))
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(10.))
        .p(px(16.))
        .rounded(px(6.))
        .bg(theme::abyss())
        .border_1()
        .border_color(theme::line_hi())
        .cursor_pointer()
        .hover(|s| s.border_color(theme::frost()).bg(theme::crypt()))
        .child(
            div()
                .w(px(32.))
                .h(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme::RADIUS)
                .bg(theme::slate())
                .text_size(theme::TEXT_DISPLAY)
                .text_color(theme::frost())
                .child(if nerd { glyph } else { "›" }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(theme::TEXT_LIST)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::bone())
                        .child(label),
                )
                .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(sub)),
        )
}

/// A quiet footer link.
fn link(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(6.))
        .py(px(2.))
        .rounded(theme::RADIUS)
        .text_size(theme::TEXT_CONTROL)
        .text_color(theme::mute())
        .cursor_pointer()
        .hover(|s| s.text_color(theme::bone()).bg(theme::ash()))
        .child(label)
}

impl Kerf {
    /// Centered start page: brand → three primary actions → recent repositories → learn links.
    pub fn render_start_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let nerd = self.nerd();
        let recents: Vec<_> = self.persisted.recents.iter().take(MAX_RECENTS).cloned().collect();

        let brand = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(14.))
            .child(super::widgets::app_icon(72.))
            .child(div().text_size(px(30.)).font_weight(FontWeight::BOLD).text_color(theme::bone()).child("Kerf"))
            .child(
                div()
                    .text_size(theme::TEXT_LIST)
                    .text_color(theme::mute())
                    .child("The cut between two branches — or any two texts."),
            );

        let actions = div()
            .flex()
            .justify_center()
            .gap(px(14.))
            .child(
                card("start-open", nerd, "\u{ea62}", "Open Repository", "Compare branches, tags and commits")
                    .on_click(cx.listener(|this, _, _, cx| this.prompt_open(cx))),
            )
            .child(
                card("start-new", nerd, "\u{ea7f}", "New Diff", "Type or paste two texts — live")
                    .on_click(cx.listener(|this, _, _, cx| this.new_scratch(cx))),
            )
            .child(
                card("start-files", nerd, "\u{eae1}", "Compare Files", "Any two files on disk")
                    .on_click(cx.listener(|this, _, _, cx| this.prompt_compare_files(cx))),
            );

        let recent = (!recents.is_empty()).then(|| {
            div().w(px(628.)).flex().flex_col().child(micro("Recent").px(px(10.)).mb(px(6.))).children(
                recents.into_iter().enumerate().map(|(i, p)| {
                    let exists = p.exists();
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let path: SharedString = tilde(&p.display().to_string()).into();
                    let (open_p, remove_p) = (p.clone(), p.clone());
                    div()
                        .id(("recent", i))
                        .group("recent")
                        .h(px(34.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(10.))
                        .rounded(theme::RADIUS)
                        .when(exists, |d| {
                            d.cursor_pointer()
                                .hover(|s| s.bg(theme::ash()))
                                .on_click(cx.listener(move |this, _, _, cx| this.open_repo(open_p.clone(), cx)))
                        })
                        .child(icon(nerd, "\u{ea62}"))
                        .child(
                            div()
                                .flex_none()
                                .text_size(theme::TEXT_LIST)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if exists { theme::bone() } else { theme::mute() })
                                .when(!exists, |d| d.line_through())
                                .child(name),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::mute())
                                .child(if exists { path } else { format!("{path} · missing").into() }),
                        )
                        .child(
                            div()
                                .id(("recent-remove", i))
                                .flex_none()
                                .w(px(22.))
                                .h(px(22.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(theme::RADIUS)
                                .text_color(theme::mute())
                                .when(exists, |d| d.invisible().group_hover("recent", |s| s.visible()))
                                .hover(|s| s.bg(theme::slate()).text_color(theme::bone()))
                                .cursor_pointer()
                                .tooltip(|_, cx| cx.new(|_| super::widgets::Tip("Remove from Recent")).into())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.persisted.recents.retain(|r| r != &remove_p);
                                    this.persisted.save();
                                    cx.notify();
                                }))
                                .child(if nerd { "\u{ea76}" } else { "✕" }),
                        )
                }),
            )
        });

        let learn = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(link("learn-keys", "Keyboard Shortcuts").on_click(cx.listener(|this, _, _, cx| {
                this.shortcuts_open = true;
                cx.notify();
            })))
            .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::faint()).child("·"))
            .child(link("learn-views", "PR Merge vs Compare View").on_click(cx.listener(|this, _, _, cx| {
                this.info_open = true;
                cx.notify();
            })));

        div()
            .id("start-page")
            .size_full()
            .overflow_y_scroll()
            .bg(theme::void())
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(40.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(36.))
                    .child(brand)
                    .when_some(self.repo_error.clone(), |d, e| {
                        d.child(
                            div()
                                .w(px(628.))
                                .px(px(12.))
                                .py(px(8.))
                                .rounded(theme::RADIUS)
                                .bg(theme::del_bg())
                                .border_1()
                                .border_color(theme::del_emph())
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::del_fg())
                                .child(e),
                        )
                    })
                    .child(actions)
                    .children(recent)
                    .child(learn),
            )
            .into_any_element()
    }

    // ───────────────────────────── repository switcher ─────────────────────────────

    pub fn open_repo_menu(&mut self, cx: &mut Context<Self>) {
        self.repo_menu_open = true;
        self.repo_query.clear();
        // Preselect the first repo that isn't the current one.
        self.repo_sel = self
            .repo_menu_items()
            .iter()
            .position(|i| matches!(i, RepoItem::Recent(p) if Some(p) != self.repo_path.as_ref()))
            .unwrap_or(0);
        self.input = super::app::Input::RepoMenu;
        cx.notify();
    }

    pub fn close_repo_menu(&mut self, cx: &mut Context<Self>) {
        self.repo_menu_open = false;
        if self.input == super::app::Input::RepoMenu {
            self.input = super::app::Input::None;
        }
        cx.notify();
    }

    /// Everything the switcher lists, in keyboard order: matching recents, then actions.
    pub fn repo_menu_items(&self) -> Vec<RepoItem> {
        let q = self.repo_query.to_lowercase();
        let mut items: Vec<RepoItem> = self
            .persisted
            .recents
            .iter()
            .filter(|p| q.is_empty() || p.display().to_string().to_lowercase().contains(&q))
            .take(MAX_RECENTS)
            .cloned()
            .map(RepoItem::Recent)
            .collect();
        items.push(RepoItem::Browse);
        if self.repo_path.is_some() && q.is_empty() {
            items.push(RepoItem::Close);
        }
        items
    }

    pub fn activate_repo_item(&mut self, item: RepoItem, cx: &mut Context<Self>) {
        self.close_repo_menu(cx);
        match item {
            RepoItem::Recent(p) if p.exists() && Some(&p) != self.repo_path.as_ref() => self.open_repo(p, cx),
            RepoItem::Recent(_) => {}
            RepoItem::Browse => self.prompt_open(cx),
            RepoItem::Close => self.close_repo(cx),
        }
    }

    /// Titlebar repository switcher: search, recents with avatars, footer actions.
    pub fn render_repo_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.repo_menu_open {
            return None;
        }
        let nerd = self.nerd();
        let items = self.repo_menu_items();
        let sel = self.repo_sel.min(items.len().saturating_sub(1));
        let recents: Vec<(usize, std::path::PathBuf)> = items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| match it {
                RepoItem::Recent(p) => Some((i, p.clone())),
                _ => None,
            })
            .collect();
        let actions: Vec<(usize, RepoItem)> = items
            .iter()
            .enumerate()
            .filter(|(_, it)| !matches!(it, RepoItem::Recent(_)))
            .map(|(i, it)| (i, it.clone()))
            .collect();

        let row = |i: usize, active: bool| {
            div()
                .id(("repo-item", i))
                .mx(px(6.))
                .px(px(8.))
                .h(px(40.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(6.))
                .cursor_pointer()
                .when(active, |d| d.bg(theme::slate()))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered && this.repo_sel != i {
                        this.repo_sel = i;
                        cx.notify();
                    }
                }))
        };

        let search = div()
            .h(px(40.))
            .mx(px(6.))
            .mt(px(6.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(6.))
            .bg(theme::abyss())
            .border_1()
            .border_color(theme::line_hi())
            .text_size(theme::TEXT_LIST)
            .child(div().text_color(theme::mute()).child(if nerd { "\u{ea6d}" } else { "⌕" }))
            .when(self.repo_query.is_empty(), |d| {
                d.child(div().text_color(theme::mute()).child("Search repositories…"))
            })
            .when(!self.repo_query.is_empty(), |d| {
                d.child(div().text_color(theme::bone()).child(self.repo_query.clone()))
            })
            .child(div().w(px(1.)).h(px(16.)).bg(theme::frost()));

        let list =
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .py(px(6.))
                .when(recents.is_empty(), |d| {
                    d.child(
                        div().px(px(16.)).py(px(10.)).text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(
                            if self.repo_query.is_empty() { "No recent repositories yet" } else { "No matches" },
                        ),
                    )
                })
                .children(recents.into_iter().map(|(i, p)| {
                    let exists = p.exists();
                    let current = self.repo_path.as_ref() == Some(&p);
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let path = tilde(&p.parent().map(|x| x.display().to_string()).unwrap_or_default());
                    let open_p = p.clone();
                    let remove_p = p.clone();
                    row(i, i == sel)
                        .group("repo-row")
                        .when(!exists, |d| d.opacity(0.55))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate_repo_item(RepoItem::Recent(open_p.clone()), cx)
                        }))
                        .child(avatar(&name))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_baseline()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_none()
                                        .text_size(theme::TEXT_LIST)
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme::bone())
                                        .when(!exists, |d| d.line_through())
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_size(theme::TEXT_CONTROL)
                                        .text_color(theme::mute())
                                        .child(if exists { path } else { "missing".into() }),
                                ),
                        )
                        .when(current, |d| {
                            d.child(
                                div()
                                    .flex_none()
                                    .text_size(theme::TEXT_LIST)
                                    .text_color(theme::frost())
                                    .child(if nerd { "\u{eab2}" } else { "✓" }),
                            )
                        })
                        .when(!current, |d| {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("repo-remove-{i}")))
                                    .flex_none()
                                    .w(px(22.))
                                    .h(px(22.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(theme::RADIUS)
                                    .text_color(theme::mute())
                                    .invisible()
                                    .group_hover("repo-row", |s| s.visible())
                                    .hover(|s| s.bg(theme::line_hi()).text_color(theme::bone()))
                                    .tooltip(|_, cx| cx.new(|_| super::widgets::Tip("Remove from Recent")).into())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.persisted.recents.retain(|r| r != &remove_p);
                                        this.persisted.save();
                                        cx.notify();
                                    }))
                                    .child(if nerd { "\u{ea76}" } else { "✕" }),
                            )
                        })
                }));

        let footer = div().flex().flex_col().gap(px(2.)).py(px(6.)).border_t_1().border_color(theme::line()).children(
            actions.into_iter().map(|(i, item)| {
                let (glyph, label, danger) = match item {
                    RepoItem::Browse => ("\u{ea83}", "Open Folder…", false),
                    _ => ("\u{ea76}", "Close Repository", true),
                };
                let it = item.clone();
                row(i, i == sel)
                    .h(px(32.))
                    .on_click(cx.listener(move |this, _, _, cx| this.activate_repo_item(it.clone(), cx)))
                    .child(div().w(px(24.)).flex().justify_center().text_color(theme::mute()).child(if nerd {
                        glyph
                    } else {
                        "›"
                    }))
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme::TEXT_LIST)
                            .text_color(if danger { theme::del_fg() } else { theme::body() })
                            .child(label),
                    )
                    .when(matches!(item, RepoItem::Browse), |d| {
                        d.child(
                            div()
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::mute())
                                .child(super::widgets::keys("⌘O")),
                        )
                    })
            }),
        );

        let left = if cfg!(target_os = "macos") { 76. } else { 8. };
        Some(
            div()
                .id("repo-menu-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_click(cx.listener(|this, _, _, cx| this.close_repo_menu(cx)))
                .child(
                    div()
                        .id("repo-menu")
                        .absolute()
                        .top(theme::TITLEBAR_H + px(6.))
                        .left(px(left))
                        .w(px(440.))
                        .max_h(px(520.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .bg(theme::crypt())
                        .border_1()
                        .border_color(theme::line_hi())
                        .rounded(px(10.))
                        .shadow_lg()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(search)
                        .child(list)
                        .child(footer),
                )
                .into_any_element(),
        )
    }

    /// Leaves git mode: back to the start page (plain-diff tabs stay open).
    pub fn close_repo(&mut self, cx: &mut Context<Self>) {
        self.repo_path = None;
        self.repo_name.clear();
        self.repo_error = None;
        self.refs = Default::default();
        self.commits = Default::default();
        self.base = None;
        self.compare = None;
        self.range = super::app::RangeState::Idle;
        self.rows.clear();
        self.selected = None;
        self.persisted.last_repo = None;
        self.persisted.save();
        // Keep plain-diff tabs; git tabs belong to the closed repo.
        let active_scratch = self.active_scratch().map(|s| s.id);
        self.tabs.retain(|t| t.target.scratch.is_some());
        self.active_tab = None;
        self.diff = super::app::DiffState::Empty;
        let idx = active_scratch
            .and_then(|id| self.tabs.iter().position(|t| t.target.scratch.as_ref().is_some_and(|s| s.id == id)))
            .or_else(|| self.tabs.len().checked_sub(1));
        if let Some(i) = idx {
            self.activate_tab(i, cx);
        }
        cx.notify();
    }
}

/// A row in the repository switcher.
#[derive(Clone, Debug, PartialEq)]
pub enum RepoItem {
    Recent(std::path::PathBuf),
    Browse,
    Close,
}

/// Letter tile for a repository: first letter on a tint picked from the name.
fn avatar(name: &str) -> Div {
    let tints =
        [theme::frost(), theme::add_fg(), theme::mod_fg(), theme::del_fg(), theme::syn_type(), theme::syn_attr()];
    let h = name.bytes().fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    let tint = tints[h % tints.len()];
    let letter = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    div()
        .w(px(26.))
        .h(px(26.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(tint.opacity(0.16))
        .border_1()
        .border_color(tint.opacity(0.35))
        .text_size(theme::TEXT_LIST)
        .font_weight(FontWeight::BOLD)
        .text_color(tint)
        .child(letter)
}
