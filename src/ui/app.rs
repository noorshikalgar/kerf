//! Root view: state, background jobs, and action handlers. Rendering lives in
//! sidebar.rs / diff_view.rs / picker.rs.

use super::state::Persisted;
use super::*;
use crate::diff::{self, Layout, Rows};
use crate::git::{
    short_sha, Change, CommitInfo, Comparison, DiffBody, DiffOptions, DiffSource, FileDiff,
    RangeMode, RangeSpec, RefInfo, RefKind, Repo,
};
use crate::highlight::{self, LineSpans};
use crate::theme;
use git2::Oid;
use gpui::{
    div, prelude::*, px, ClipboardItem, Context, FocusHandle, KeyDownEvent, MouseButton,
    MouseMoveEvent, MouseUpEvent, PathPromptOptions, ScrollStrategy, SharedString, Task,
    UniformListScrollHandle, Window,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Files,
    Commits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Base,
    Compare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Ahead,
    Behind,
}

pub struct RangeData {
    pub cmp: Comparison,
    pub changes: Arc<Vec<Change>>,
    pub additions: u64,
    pub deletions: u64,
}

pub enum RangeState {
    Idle,
    Loading,
    Ready(Arc<RangeData>),
    Error(String),
}

/// Sidebar list row.
#[derive(Debug, Clone)]
pub enum ListRow {
    Summary,
    Dir { path: String, name: String, depth: usize, collapsed: bool, files: usize },
    File { change: usize, depth: usize },
    Group { group: Group, count: usize, open: bool },
    Commit { group: Group, idx: usize },
    CommitFile { change: usize },
    Notice(SharedString),
}

impl ListRow {
    pub fn selectable(&self) -> bool {
        !matches!(self, ListRow::Summary | ListRow::Notice(_))
    }
}

pub struct ExpandedCommit {
    pub oid: Oid,
    pub source: Option<DiffSource>,
    pub changes: Option<Arc<Vec<Change>>>,
    pub error: Option<String>,
}

/// What the diff pane is showing.
#[derive(Clone)]
pub struct Target {
    pub source: DiffSource,
    pub change: Change,
    pub commit: Option<CommitInfo>,
}

impl Target {
    pub fn key(&self) -> String {
        format!("{:?}:{:?}:{}", self.source.old, self.source.new, self.change.path)
    }
}

pub struct Loaded {
    pub target: Target,
    pub fd: Arc<FileDiff>,
    pub rows: Arc<Rows>,
    pub hl: Option<Arc<Vec<LineSpans>>>,
    /// Row with the longest text, used for horizontal sizing.
    pub widest_row: Option<usize>,
    pub gutter_digits: usize,
}

pub enum DiffState {
    Empty,
    Loading { target: Target, prev: Option<Arc<Loaded>>, since: Instant },
    Ready(Arc<Loaded>),
    Error { target: Target, message: String },
}

/// One entry in the ref picker.
#[derive(Debug, Clone, PartialEq)]
pub enum PickItem {
    Ref(usize),
    Commit(usize),
    /// Free-form revision typed by the user (SHA, `HEAD~3`, …); validated on load.
    Raw(String),
}

/// What a Base/Compare value points at, for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefLook {
    Branch,
    Remote,
    Tag,
    Commit,
    Rev,
}

pub struct Described {
    pub look: RefLook,
    /// Branch/tag name, or short SHA for commits.
    pub name: String,
    /// Commit subject when the value is a commit.
    pub subject: Option<String>,
    pub sha: Option<String>,
}

pub struct Picker {
    pub which: Which,
    pub query: String,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    None,
    Filter,
    Picker,
}

pub struct Kerf {
    pub focus: FocusHandle,
    pub font: SharedString,
    pub persisted: Persisted,

    pub repo_path: Option<PathBuf>,
    pub repo_name: String,
    pub repo_error: Option<String>,
    pub repo_loading: bool,
    pub refs: Arc<Vec<RefInfo>>,
    pub commits: Arc<Vec<CommitInfo>>,
    pub base: Option<String>,
    pub compare: Option<String>,
    pub mode: RangeMode,

    pub range: RangeState,
    range_gen: u64,
    range_task: Option<Task<()>>,

    pub tab: Tab,
    pub tree: bool,
    pub filter: String,
    pub input: Input,
    pub collapsed_dirs: HashSet<String>,
    pub ahead_open: bool,
    pub behind_open: bool,
    pub expanded: Option<ExpandedCommit>,
    commit_task: Option<Task<()>>,
    pub rows: Vec<ListRow>,
    pub selected: Option<usize>,
    pub list_scroll: UniformListScrollHandle,
    pub viewed: HashSet<String>,

    pub diff: DiffState,
    diff_gen: u64,
    diff_task: Option<Task<()>>,
    hl_task: Option<Task<()>>,
    pub layout: Layout,
    pub ignore_ws: bool,
    pub forced: HashSet<String>,
    pub shown_generated: HashSet<String>,
    pub full_context: HashSet<String>,
    pub diff_scroll: UniformListScrollHandle,

    pub picker: Option<Picker>,
    pub sidebar_open: bool,
    pub sidebar_w: f32,
    pub resizing: bool,
    pub scroll_drag: Option<super::scrollbar::Drag>,
    pub flash: Option<(SharedString, Instant)>,
    flash_task: Option<Task<()>>,
}

impl Kerf {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let persisted = Persisted::load();
        let names = window.text_system().all_font_names();
        let font = theme::FONT_CANDIDATES
            .iter()
            .find(|c| names.iter().any(|n| n == *c))
            .copied()
            .unwrap_or("Menlo");
        let focus = cx.focus_handle();
        window.focus(&focus);
        let mut this = Self {
            focus,
            font: font.into(),
            layout: if persisted.split { Layout::Split } else { Layout::Unified },
            ignore_ws: persisted.ignore_ws,
            tree: persisted.tree,
            sidebar_w: persisted.sidebar_w.unwrap_or(theme::SIDEBAR_W),
            persisted,
            repo_path: None,
            repo_name: String::new(),
            repo_error: None,
            repo_loading: false,
            refs: Arc::new(Vec::new()),
            commits: Arc::new(Vec::new()),
            base: None,
            compare: None,
            mode: RangeMode::ThreeDot,
            range: RangeState::Idle,
            range_gen: 0,
            range_task: None,
            tab: Tab::Files,
            filter: String::new(),
            input: Input::None,
            collapsed_dirs: HashSet::new(),
            ahead_open: true,
            behind_open: false,
            expanded: None,
            commit_task: None,
            rows: Vec::new(),
            selected: None,
            list_scroll: UniformListScrollHandle::new(),
            viewed: HashSet::new(),
            diff: DiffState::Empty,
            diff_gen: 0,
            diff_task: None,
            hl_task: None,
            forced: HashSet::new(),
            shown_generated: HashSet::new(),
            full_context: HashSet::new(),
            diff_scroll: UniformListScrollHandle::new(),
            picker: None,
            sidebar_open: true,
            resizing: false,
            scroll_drag: None,
            flash: None,
            flash_task: None,
        };
        let start = path.or_else(|| this.persisted.last_repo.clone());
        if let Some(p) = start {
            this.open_repo(p, cx);
        }
        this
    }

    // ───────────────────────────── repo ─────────────────────────────

    pub fn prompt_open(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Repository".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(p) = paths.into_iter().next() {
                    this.update(cx, |this, cx| this.open_repo(p, cx)).ok();
                }
            }
        })
        .detach();
    }

    pub fn open_repo(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.repo_loading = true;
        self.repo_error = None;
        let remembered = self.persisted.ranges.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let repo = Repo::open(&path)?;
                    let refs = repo.refs()?;
                    let commits = repo.recent_commits(RECENT_COMMITS)?;
                    let root = repo.root().to_path_buf();
                    let key = root.display().to_string();
                    let range = remembered
                        .get(&key)
                        .filter(|(b, c)| repo.resolve(b).is_ok() && repo.resolve(c).is_ok())
                        .cloned()
                        .or_else(|| repo.default_range(&refs));
                    anyhow::Ok((root, repo.name(), refs, commits, range, repo.is_empty()))
                })
                .await;
            this.update(cx, |this, cx| {
                this.repo_loading = false;
                match result {
                    Ok((root, name, refs, commits, range, empty)) => {
                        this.commits = Arc::new(commits);
                        this.persisted.push_recent(root.clone());
                        this.persisted.save();
                        this.repo_path = Some(root);
                        this.repo_name = name;
                        this.refs = Arc::new(refs);
                        this.reset_view();
                        if empty {
                            this.repo_error = Some("No commits yet".into());
                        }
                        if let Some((b, c)) = range {
                            this.base = Some(b);
                            this.compare = Some(c);
                        } else {
                            this.base = None;
                            this.compare = None;
                        }
                        this.load_range(cx);
                    }
                    Err(e) => this.repo_error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        });
        self.range_task = Some(task);
        cx.notify();
    }

    fn reset_view(&mut self) {
        self.range = RangeState::Idle;
        self.diff = DiffState::Empty;
        self.expanded = None;
        self.viewed.clear();
        self.collapsed_dirs.clear();
        self.selected = None;
        self.filter.clear();
        self.rows.clear();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path.clone() else { return };
        let task = cx.spawn(async move |this, cx| {
            let refs = cx
                .background_executor()
                .spawn(async move {
                    let r = Repo::open(&path)?;
                    anyhow::Ok((r.refs()?, r.recent_commits(RECENT_COMMITS)?))
                })
                .await;
            this.update(cx, |this, cx| {
                if let Ok((refs, commits)) = refs {
                    this.refs = Arc::new(refs);
                    this.commits = Arc::new(commits);
                }
                this.load_range(cx);
            })
            .ok();
        });
        self.range_task = Some(task);
        self.flash("Refreshed", cx);
    }

    // ───────────────────────────── range ─────────────────────────────

    pub fn load_range(&mut self, cx: &mut Context<Self>) {
        let (Some(path), Some(base), Some(compare)) =
            (self.repo_path.clone(), self.base.clone(), self.compare.clone())
        else {
            self.range = RangeState::Idle;
            self.rebuild_rows();
            cx.notify();
            return;
        };
        if let Some(p) = &self.repo_path {
            self.persisted
                .ranges
                .insert(p.display().to_string(), (base.clone(), compare.clone()));
            self.persisted.save();
        }
        self.range_gen += 1;
        let generation = self.range_gen;
        let spec = RangeSpec { base, compare, mode: self.mode };
        self.range = RangeState::Loading;
        self.rebuild_rows();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let repo = Repo::open(&path)?;
                    let cmp = repo.compare(&spec)?;
                    let changes = repo.changes(cmp.source)?;
                    let additions = changes.iter().filter_map(|c| c.additions).map(u64::from).sum();
                    let deletions = changes.iter().filter_map(|c| c.deletions).map(u64::from).sum();
                    anyhow::Ok(RangeData { cmp, changes: Arc::new(changes), additions, deletions })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.range_gen != generation {
                    return;
                }
                let prev_target = this.current_target().map(|t| t.change.path.clone());
                this.range = match result {
                    Ok(data) => RangeState::Ready(Arc::new(data)),
                    Err(e) => RangeState::Error(e.to_string()),
                };
                this.expanded = None;
                this.rebuild_rows();
                // Keep the same file selected across refreshes when it still exists.
                let reselect = prev_target.and_then(|p| this.row_for_path(&p));
                let first = reselect.or_else(|| this.rows.iter().position(|r| matches!(r, ListRow::File { .. })));
                match first {
                    Some(ix) if this.tab == Tab::Files => this.select_row(ix, cx),
                    _ => {
                        if this.tab == Tab::Files {
                            this.diff = DiffState::Empty;
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        });
        self.range_task = Some(task);
        cx.notify();
    }

    pub fn range_data(&self) -> Option<&Arc<RangeData>> {
        match &self.range {
            RangeState::Ready(d) => Some(d),
            _ => None,
        }
    }

    pub fn set_ref(&mut self, which: Which, name: String, cx: &mut Context<Self>) {
        match which {
            Which::Base => self.base = Some(name),
            Which::Compare => self.compare = Some(name),
        }
        self.viewed.clear();
        self.load_range(cx);
    }

    pub fn swap(&mut self, cx: &mut Context<Self>) {
        std::mem::swap(&mut self.base, &mut self.compare);
        self.viewed.clear();
        self.load_range(cx);
    }

    pub fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        if self.range_data().is_some_and(|d| d.cmp.unrelated()) {
            return;
        }
        self.mode = match self.mode {
            RangeMode::ThreeDot => RangeMode::TwoDot,
            RangeMode::TwoDot => RangeMode::ThreeDot,
        };
        self.load_range(cx);
    }

    // ───────────────────────────── sidebar rows ─────────────────────────────

    pub fn rebuild_rows(&mut self) {
        let prev = self.selected.and_then(|i| self.rows.get(i)).map(row_identity);
        self.rows = match self.tab {
            Tab::Files => self.file_rows(),
            Tab::Commits => self.commit_rows(),
        };
        self.selected = prev
            .and_then(|id| self.rows.iter().position(|r| row_identity(r) == id))
            .or(None);
    }

    fn file_rows(&self) -> Vec<ListRow> {
        let Some(data) = self.range_data() else { return Vec::new() };
        let mut rows = vec![ListRow::Summary];
        let q = self.filter.to_lowercase();
        let matches = |c: &Change| {
            q.is_empty()
                || c.path.to_lowercase().contains(&q)
                || c.old_path.as_ref().is_some_and(|o| o.to_lowercase().contains(&q))
        };
        let visible: Vec<usize> = (0..data.changes.len()).filter(|&i| matches(&data.changes[i])).collect();
        if data.cmp.identical() {
            rows.push(ListRow::Notice("Identical — nothing to compare".into()));
            return rows;
        }
        if visible.is_empty() {
            rows.push(ListRow::Notice(if q.is_empty() {
                "No file changes".into()
            } else {
                "No files match filter".into()
            }));
            return rows;
        }
        if !self.tree || !q.is_empty() {
            rows.extend(visible.into_iter().map(|change| ListRow::File { change, depth: 0 }));
            return rows;
        }
        rows.extend(tree_rows(&data.changes, &visible, &self.collapsed_dirs));
        rows
    }

    fn commit_rows(&self) -> Vec<ListRow> {
        let Some(data) = self.range_data() else { return Vec::new() };
        let mut rows = Vec::new();
        for (group, list, open) in [
            (Group::Ahead, &data.cmp.ahead, self.ahead_open),
            (Group::Behind, &data.cmp.behind, self.behind_open),
        ] {
            rows.push(ListRow::Group { group, count: list.len(), open });
            if !open {
                continue;
            }
            if list.is_empty() {
                rows.push(ListRow::Notice(match group {
                    Group::Ahead => "No commits ahead".into(),
                    Group::Behind => "No commits behind".into(),
                }));
            }
            for (idx, c) in list.iter().enumerate() {
                rows.push(ListRow::Commit { group, idx });
                if let Some(exp) = self.expanded.as_ref().filter(|e| e.oid == c.oid) {
                    match (&exp.changes, &exp.error) {
                        (Some(ch), _) => {
                            if ch.is_empty() {
                                rows.push(ListRow::Notice("Empty commit".into()));
                            }
                            rows.extend((0..ch.len()).map(|change| ListRow::CommitFile { change }));
                        }
                        (None, Some(e)) => rows.push(ListRow::Notice(e.clone().into())),
                        (None, None) => rows.push(ListRow::Notice("Loading…".into())),
                    }
                }
            }
        }
        rows
    }

    fn row_for_path(&self, path: &str) -> Option<usize> {
        let data = self.range_data()?;
        self.rows.iter().position(|r| match r {
            ListRow::File { change, .. } => data.changes[*change].path == path,
            _ => false,
        })
    }

    pub fn commit(&self, group: Group, idx: usize) -> Option<&CommitInfo> {
        let d = self.range_data()?;
        match group {
            Group::Ahead => d.cmp.ahead.get(idx),
            Group::Behind => d.cmp.behind.get(idx),
        }
    }

    pub fn select_row(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(ix).cloned() else { return };
        if !row.selectable() {
            return;
        }
        self.selected = Some(ix);
        self.list_scroll.scroll_to_item(ix, ScrollStrategy::Center);
        match row {
            ListRow::File { change, .. } => {
                if let Some(d) = self.range_data().cloned() {
                    self.show(
                        Target { source: d.cmp.source, change: d.changes[change].clone(), commit: None },
                        cx,
                    );
                }
            }
            ListRow::Commit { group, idx } => {
                if let Some(c) = self.commit(group, idx).cloned() {
                    self.expand_commit(c, cx);
                }
            }
            ListRow::CommitFile { change } => {
                if let Some(exp) = &self.expanded {
                    if let (Some(src), Some(ch)) = (exp.source, exp.changes.clone()) {
                        let commit = self.find_commit(exp.oid);
                        self.show(Target { source: src, change: ch[change].clone(), commit }, cx);
                    }
                }
            }
            ListRow::Dir { .. } | ListRow::Group { .. } => {}
            ListRow::Summary | ListRow::Notice(_) => {}
        }
        cx.notify();
    }

    fn find_commit(&self, oid: Oid) -> Option<CommitInfo> {
        let d = self.range_data()?;
        d.cmp.ahead.iter().chain(d.cmp.behind.iter()).find(|c| c.oid == oid).cloned()
    }

    fn expand_commit(&mut self, commit: CommitInfo, cx: &mut Context<Self>) {
        if self.expanded.as_ref().is_some_and(|e| e.oid == commit.oid) {
            // Already expanded: show first file.
            self.show_first_commit_file(cx);
            return;
        }
        let Some(path) = self.repo_path.clone() else { return };
        let oid = commit.oid;
        self.expanded = Some(ExpandedCommit { oid, source: None, changes: None, error: None });
        self.rebuild_rows();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { Repo::open(&path)?.commit_changes(oid) })
                .await;
            this.update(cx, |this, cx| {
                let Some(exp) = this.expanded.as_mut().filter(|e| e.oid == oid) else { return };
                match result {
                    Ok((src, changes)) => {
                        exp.source = Some(src);
                        exp.changes = Some(Arc::new(changes));
                    }
                    Err(e) => exp.error = Some(e.to_string()),
                }
                this.rebuild_rows();
                this.show_first_commit_file(cx);
                cx.notify();
            })
            .ok();
        });
        self.commit_task = Some(task);
    }

    fn show_first_commit_file(&mut self, cx: &mut Context<Self>) {
        let Some(exp) = &self.expanded else { return };
        let (Some(src), Some(ch)) = (exp.source, exp.changes.clone()) else { return };
        let commit = self.find_commit(exp.oid);
        if let Some(first) = ch.first() {
            self.show(Target { source: src, change: first.clone(), commit }, cx);
        } else {
            self.diff = DiffState::Empty;
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len() as isize;
        let mut ix = self.selected.map(|s| s as isize).unwrap_or(-1);
        loop {
            ix += delta.signum();
            if ix < 0 || ix >= len {
                return;
            }
            if self.rows[ix as usize].selectable() {
                break;
            }
        }
        self.select_row(ix as usize, cx);
    }

    /// Jump to the next/previous *file* row (skips dirs and commits).
    fn step_file(&mut self, forward: bool, cx: &mut Context<Self>) {
        let len = self.rows.len();
        let start = self.selected.unwrap_or(if forward { usize::MAX } else { len });
        let is_file = |r: &ListRow| matches!(r, ListRow::File { .. } | ListRow::CommitFile { .. });
        let found = if forward {
            (start.wrapping_add(1)..len).find(|&i| is_file(&self.rows[i]))
        } else {
            (0..start.min(len)).rev().find(|&i| is_file(&self.rows[i]))
        };
        if let Some(i) = found {
            self.select_row(i, cx);
        }
    }

    fn toggle_row(&mut self, expand: Option<bool>, cx: &mut Context<Self>) {
        let Some(ix) = self.selected else { return };
        match self.rows.get(ix).cloned() {
            Some(ListRow::Dir { path, collapsed, .. }) => {
                let want_collapsed = match expand {
                    Some(e) => !e,
                    None => !collapsed,
                };
                if want_collapsed {
                    self.collapsed_dirs.insert(path);
                } else {
                    self.collapsed_dirs.remove(&path);
                }
                self.rebuild_rows();
            }
            Some(ListRow::Group { group, open, .. }) => {
                let v = expand.unwrap_or(!open);
                match group {
                    Group::Ahead => self.ahead_open = v,
                    Group::Behind => self.behind_open = v,
                }
                self.rebuild_rows();
            }
            Some(ListRow::Commit { group, idx }) => {
                let oid = self.commit(group, idx).map(|c| c.oid);
                let is_open = self.expanded.as_ref().map(|e| Some(e.oid) == oid).unwrap_or(false);
                if expand == Some(false) || (expand.is_none() && is_open) {
                    self.expanded = None;
                    self.rebuild_rows();
                } else if let Some(c) = self.commit(group, idx).cloned() {
                    self.expand_commit(c, cx);
                }
            }
            Some(ListRow::File { .. }) | Some(ListRow::CommitFile { .. }) if expand == Some(false) => {
                // ← on a file: jump to its parent dir / commit row.
                if let Some(parent) = (0..ix).rev().find(|&i| match (&self.rows[ix], &self.rows[i]) {
                    (ListRow::File { depth, .. }, ListRow::Dir { depth: d, .. }) => *d + 1 == *depth,
                    (ListRow::CommitFile { .. }, ListRow::Commit { .. }) => true,
                    _ => false,
                }) {
                    self.selected = Some(parent);
                    self.list_scroll.scroll_to_item(parent, ScrollStrategy::Center);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn click_row(&mut self, ix: usize, cx: &mut Context<Self>) {
        let was_selected = self.selected == Some(ix);
        match self.rows.get(ix) {
            Some(ListRow::Dir { .. }) | Some(ListRow::Group { .. }) => {
                self.selected = Some(ix);
                self.toggle_row(None, cx);
            }
            Some(ListRow::Commit { .. }) if was_selected => self.toggle_row(None, cx),
            _ => self.select_row(ix, cx),
        }
    }

    pub fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.selected = None;
        self.rebuild_rows();
        let first = match tab {
            Tab::Files => self.rows.iter().position(|r| matches!(r, ListRow::File { .. })),
            Tab::Commits => self.rows.iter().position(|r| matches!(r, ListRow::Commit { .. })),
        };
        if let Some(i) = first {
            if tab == Tab::Files {
                self.select_row(i, cx);
            } else {
                self.selected = Some(i);
            }
        }
        cx.notify();
    }

    // ───────────────────────────── diff ─────────────────────────────

    /// The diff currently on screen (ready, or the previous one while loading).
    pub fn loaded(&self) -> Option<&Arc<Loaded>> {
        match &self.diff {
            DiffState::Ready(l) => Some(l),
            DiffState::Loading { prev, .. } => prev.as_ref(),
            _ => None,
        }
    }

    pub fn current_target(&self) -> Option<&Target> {
        match &self.diff {
            DiffState::Empty => None,
            DiffState::Loading { target, .. } | DiffState::Error { target, .. } => Some(target),
            DiffState::Ready(l) => Some(&l.target),
        }
    }

    pub fn show(&mut self, target: Target, cx: &mut Context<Self>) {
        if let DiffState::Ready(l) = &self.diff {
            if l.target.key() == target.key() && l.target.commit.as_ref().map(|c| c.oid) == target.commit.as_ref().map(|c| c.oid) {
                return;
            }
        }
        self.load_diff(target, true, cx);
    }

    fn reload_diff(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.current_target().cloned() {
            self.load_diff(t, false, cx);
        }
    }

    fn load_diff(&mut self, target: Target, reset_scroll: bool, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path.clone() else { return };
        self.diff_gen += 1;
        let generation = self.diff_gen;
        let key = target.key();
        let opts = DiffOptions {
            ignore_whitespace: self.ignore_ws,
            force: self.forced.contains(&key),
            context_lines: if self.full_context.contains(&key) { 1_000_000 } else { 3 },
            ..Default::default()
        };
        let layout = self.layout;
        let prev = match std::mem::replace(&mut self.diff, DiffState::Empty) {
            DiffState::Ready(l) => Some(l),
            DiffState::Loading { prev, .. } => prev,
            _ => None,
        };
        self.diff = DiffState::Loading { target: target.clone(), prev, since: Instant::now() };
        self.hl_task = None;

        let change = target.change.clone();
        let task = cx.spawn(async move |this, cx| {
            let job = cx.background_executor().spawn(async move {
                let repo = Repo::open(&path)?;
                let fd = repo.file_diff(&change, opts)?;
                let rows = diff::build(&fd, layout);
                let widest_row = widest(&fd, &rows);
                let max_no = fd.lines.iter().filter_map(|l| l.old_no.max(l.new_no)).max().unwrap_or(1);
                anyhow::Ok((Arc::new(fd), Arc::new(rows), widest_row, digits(max_no)))
            });
            // Spinner only appears if the job takes longer than 150ms.
            let timer = cx.background_executor().timer(Duration::from_millis(150));
            let tick = this.clone();
            let mut tick_cx = cx.clone();
            cx.foreground_executor()
                .spawn(async move {
                    timer.await;
                    tick.update(&mut tick_cx, |_, cx| cx.notify()).ok();
                })
                .detach();
            let result = job.await;
            this.update(cx, |this, cx| {
                if this.diff_gen != generation {
                    return;
                }
                match result {
                    Ok((fd, rows, widest_row, gutter_digits)) => {
                        let loaded = Arc::new(Loaded { target: target.clone(), fd, rows, hl: None, widest_row, gutter_digits });
                        this.diff = DiffState::Ready(loaded.clone());
                        if reset_scroll {
                            this.diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        }
                        this.mark_viewed(&target);
                        this.start_highlight(loaded, generation, cx);
                    }
                    Err(e) => this.diff = DiffState::Error { target: target.clone(), message: e.to_string() },
                }
                cx.notify();
            })
            .ok();
        });
        self.diff_task = Some(task);
        cx.notify();
    }

    fn start_highlight(&mut self, loaded: Arc<Loaded>, generation: u64, cx: &mut Context<Self>) {
        if !matches!(loaded.fd.body, DiffBody::Text) || loaded.fd.lines.is_empty() {
            return;
        }
        let fd = loaded.fd.clone();
        let task = cx.spawn(async move |this, cx| {
            let hl = cx.background_executor().spawn(async move { highlight::highlight(&fd) }).await;
            let Some(hl) = hl else { return };
            this.update(cx, |this, cx| {
                if this.diff_gen != generation {
                    return;
                }
                if let DiffState::Ready(l) = &this.diff {
                    this.diff = DiffState::Ready(Arc::new(Loaded {
                        target: l.target.clone(),
                        fd: l.fd.clone(),
                        rows: l.rows.clone(),
                        hl: Some(Arc::new(hl)),
                        widest_row: l.widest_row,
                        gutter_digits: l.gutter_digits,
                    }));
                    cx.notify();
                }
            })
            .ok();
        });
        self.hl_task = Some(task);
    }

    fn mark_viewed(&mut self, target: &Target) {
        if target.commit.is_none() {
            self.viewed.insert(target.change.path.clone());
        }
    }

    pub fn force_load(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.current_target().cloned() {
            self.forced.insert(t.key());
            self.shown_generated.insert(t.key());
            self.load_diff(t, true, cx);
        }
    }

    pub fn expand_context(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.current_target().cloned() {
            self.full_context.insert(t.key());
            self.load_diff(t, false, cx);
        }
    }

    pub fn set_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        if self.layout == layout {
            return;
        }
        self.layout = layout;
        self.persisted.split = layout == Layout::Split;
        self.persisted.save();
        self.reload_diff(cx);
    }

    pub fn toggle_ws(&mut self, cx: &mut Context<Self>) {
        self.ignore_ws = !self.ignore_ws;
        self.persisted.ignore_ws = self.ignore_ws;
        self.persisted.save();
        self.reload_diff(cx);
        cx.notify();
    }

    fn jump_hunk(&mut self, forward: bool, cx: &mut Context<Self>) {
        let DiffState::Ready(l) = &self.diff else { return };
        let state = self.diff_scroll.0.borrow();
        let offset = -state.base_handle.offset().y;
        drop(state);
        let current = (offset / theme::ROW_CODE).floor() as usize;
        let target = if forward {
            l.rows.hunk_rows.iter().copied().find(|&r| r > current)
        } else {
            l.rows.hunk_rows.iter().rev().copied().find(|&r| r + 1 < current.max(1))
        };
        if let Some(r) = target {
            self.diff_scroll.scroll_to_item_strict(r, ScrollStrategy::Top);
            cx.notify();
        } else if forward {
            self.step_file(true, cx);
        }
    }

    fn page(&mut self, rows: isize, cx: &mut Context<Self>) {
        let DiffState::Ready(l) = &self.diff else { return };
        let len = l.rows.rows.len();
        if len == 0 {
            return;
        }
        let state = self.diff_scroll.0.borrow();
        let offset = -state.base_handle.offset().y;
        drop(state);
        let current = (offset / theme::ROW_CODE).floor() as isize;
        let target = (current + rows).clamp(0, len as isize - 1) as usize;
        self.diff_scroll.scroll_to_item_strict(target, ScrollStrategy::Top);
        cx.notify();
    }

    // ───────────────────────────── picker / input ─────────────────────────────

    pub fn open_picker(&mut self, which: Which, cx: &mut Context<Self>) {
        if self.refs.is_empty() {
            return;
        }
        self.picker = Some(Picker { which, query: String::new(), selected: 0 });
        self.input = Input::Picker;
        cx.notify();
    }

    /// Picker entries for the current query. Positions index chars of the entry's label
    /// (ref name, or `"<short> <subject>"` for commits).
    pub fn picker_matches(&self) -> Vec<(PickItem, Vec<usize>)> {
        let Some(p) = &self.picker else { return Vec::new() };
        picker_items(&p.query, &self.refs, &self.commits)
    }

    pub fn pick(&mut self, item: PickItem, cx: &mut Context<Self>) {
        let Some(p) = self.picker.take() else { return };
        self.input = Input::None;
        let value = match item {
            PickItem::Ref(i) => self.refs.get(i).map(|r| r.name.clone()),
            PickItem::Commit(i) => self.commits.get(i).map(|c| c.oid.to_string()),
            PickItem::Raw(s) => Some(s),
        };
        if let Some(v) = value {
            self.set_ref(p.which, v, cx);
        }
        cx.notify();
    }

    /// Sets the selected commit (Commits tab) as base or compare.
    fn set_selected_commit(&mut self, which: Which, cx: &mut Context<Self>) {
        let Some(ListRow::Commit { group, idx }) = self.selected.and_then(|i| self.rows.get(i)).cloned() else { return };
        let Some(c) = self.commit(group, idx).cloned() else { return };
        self.set_ref(which, c.oid.to_string(), cx);
        let label = if which == Which::Base { "Base" } else { "Compare" };
        self.flash(format!("{label} → {} {}", c.short(), c.summary), cx);
    }

    /// How to show a Base/Compare value: branch/tag name, or commit SHA + subject.
    pub fn describe(&self, value: &str) -> Described {
        if let Some(r) = self.refs.iter().find(|r| r.name == value) {
            let look = match r.kind {
                RefKind::Local => RefLook::Branch,
                RefKind::Remote => RefLook::Remote,
                RefKind::Tag => RefLook::Tag,
            };
            return Described { look, name: r.name.clone(), subject: None, sha: Some(short_sha(r.target)) };
        }
        if is_hex(value) && value.len() >= 7 {
            let c = self.commits.iter().find(|c| c.oid.to_string().starts_with(&value.to_lowercase()));
            let short = value[..7].to_string();
            return Described {
                look: RefLook::Commit,
                name: short.clone(),
                subject: c.map(|c| c.summary.clone()),
                sha: Some(short),
            };
        }
        Described { look: RefLook::Rev, name: value.to_string(), subject: None, sha: None }
    }

    pub fn nerd(&self) -> bool {
        self.font.contains("Nerd")
    }

    /// Short display form used in the titlebar / status bar.
    pub fn display_ref(&self, value: &str) -> String {
        self.describe(value).name
    }

    fn close_input(&mut self, cx: &mut Context<Self>) {
        match self.input {
            Input::Picker => self.picker = None,
            Input::Filter => {}
            Input::None => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.rebuild_rows();
                }
            }
        }
        self.input = Input::None;
        cx.notify();
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.input == Input::None {
            return;
        }
        let ks = &ev.keystroke;
        if ks.modifiers.platform || ks.modifiers.control {
            return;
        }
        let mut changed = false;
        let text = match self.input {
            Input::Picker => self.picker.as_mut().map(|p| &mut p.query),
            Input::Filter => Some(&mut self.filter),
            Input::None => None,
        };
        let Some(text) = text else { return };
        if ks.key == "backspace" {
            if ks.modifiers.alt {
                text.clear();
            } else {
                text.pop();
            }
            changed = true;
        } else if let Some(ch) = ks.key_char.as_ref().filter(|c| !c.chars().any(char::is_control)) {
            text.push_str(ch);
            changed = true;
        }
        if changed {
            cx.stop_propagation();
            match self.input {
                Input::Picker => {
                    if let Some(p) = self.picker.as_mut() {
                        p.selected = 0;
                    }
                }
                Input::Filter => {
                    self.selected = None;
                    self.rebuild_rows();
                }
                Input::None => {}
            }
            cx.notify();
        }
    }

    pub fn flash(&mut self, msg: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.flash = Some((msg.into(), Instant::now()));
        let t = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1600)).await;
            this.update(cx, |this, cx| {
                this.flash = None;
                cx.notify();
            })
            .ok();
        });
        self.flash_task = Some(t);
        cx.notify();
    }

    fn copy_item(&mut self, cx: &mut Context<Self>) {
        let text = match self.selected.and_then(|i| self.rows.get(i)) {
            Some(ListRow::Commit { group, idx }) => self.commit(*group, *idx).map(|c| c.oid.to_string()),
            Some(ListRow::File { change, .. }) => self.range_data().map(|d| d.changes[*change].path.clone()),
            Some(ListRow::CommitFile { change }) => self
                .expanded
                .as_ref()
                .and_then(|e| e.changes.as_ref())
                .map(|c| c[*change].path.clone()),
            _ => None,
        };
        if let Some(t) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(t.clone()));
            self.flash(format!("Copied {t}"), cx);
        }
    }

    fn update_title(&self, window: &mut Window) {
        let title = match (&self.repo_path, &self.base, &self.compare) {
            (Some(_), Some(b), Some(c)) => format!("{} — {} … {}", self.repo_name, self.display_ref(b), self.display_ref(c)),
            (Some(_), _, _) => self.repo_name.clone(),
            _ => "Kerf".into(),
        };
        window.set_window_title(&title);
    }
}

impl Render for Kerf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_title(window);
        let context = if self.input == Input::None { "Kerf" } else { "KerfInput" };
        div()
            .id("kerf")
            .key_context(context)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::void())
            .text_color(theme::bone())
            .font_family(self.font.clone())
            .text_size(theme::TEXT_CODE)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_action(cx.listener(|this, _: &OpenRepo, _, cx| this.prompt_open(cx)))
            .on_action(cx.listener(|this, _: &Refresh, _, cx| this.refresh(cx)))
            .on_action(cx.listener(|this, _: &PickBase, _, cx| this.open_picker(Which::Base, cx)))
            .on_action(cx.listener(|this, _: &PickCompare, _, cx| this.open_picker(Which::Compare, cx)))
            .on_action(cx.listener(|this, _: &Swap, _, cx| this.swap(cx)))
            .on_action(cx.listener(|this, _: &ToggleMode, _, cx| this.toggle_mode(cx)))
            .on_action(cx.listener(|this, _: &FilesTab, _, cx| this.set_tab(Tab::Files, cx)))
            .on_action(cx.listener(|this, _: &CommitsTab, _, cx| this.set_tab(Tab::Commits, cx)))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar_open = !this.sidebar_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Up, _, cx| {
                if let Some(p) = this.picker.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                    cx.notify();
                } else {
                    this.move_selection(-1, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Down, _, cx| {
                let n = this.picker_matches().len();
                if let Some(p) = this.picker.as_mut() {
                    p.selected = (p.selected + 1).min(n.saturating_sub(1));
                    cx.notify();
                } else {
                    this.move_selection(1, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Left, _, cx| this.toggle_row(Some(false), cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.toggle_row(Some(true), cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| match this.input {
                Input::Picker => {
                    let sel = this.picker.as_ref().map(|p| p.selected).unwrap_or(0);
                    if let Some((item, _)) = this.picker_matches().get(sel).cloned() {
                        this.pick(item, cx);
                    }
                }
                Input::Filter => {
                    this.input = Input::None;
                    if let Some(i) = this.rows.iter().position(|r| matches!(r, ListRow::File { .. })) {
                        this.select_row(i, cx);
                    }
                    cx.notify();
                }
                Input::None => {
                    if matches!(this.diff, DiffState::Ready(ref l) if matches!(l.fd.body, DiffBody::TooLarge))
                        || this.showing_collapsed_generated()
                    {
                        this.force_load(cx);
                    } else {
                        this.toggle_row(None, cx);
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &Cancel, _, cx| this.close_input(cx)))
            .on_action(cx.listener(|this, _: &NextFile, _, cx| this.step_file(true, cx)))
            .on_action(cx.listener(|this, _: &PrevFile, _, cx| this.step_file(false, cx)))
            .on_action(cx.listener(|this, _: &NextHunk, _, cx| this.jump_hunk(true, cx)))
            .on_action(cx.listener(|this, _: &PrevHunk, _, cx| this.jump_hunk(false, cx)))
            .on_action(cx.listener(|this, _: &ToggleSplit, _, cx| {
                let next = if this.layout == Layout::Split { Layout::Unified } else { Layout::Split };
                this.set_layout(next, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleWhitespace, _, cx| this.toggle_ws(cx)))
            .on_action(cx.listener(|this, _: &ToggleTree, _, cx| {
                this.tree = !this.tree;
                this.persisted.tree = this.tree;
                this.persisted.save();
                this.rebuild_rows();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CopyItem, _, cx| this.copy_item(cx)))
            .on_action(cx.listener(|this, _: &SetBase, _, cx| this.set_selected_commit(Which::Base, cx)))
            .on_action(cx.listener(|this, _: &SetCompare, _, cx| this.set_selected_commit(Which::Compare, cx)))
            .on_action(cx.listener(|this, _: &FocusFilter, _, cx| {
                if this.tab == Tab::Files {
                    this.input = Input::Filter;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.page(30, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.page(-30, cx)))
            .on_action(cx.listener(|this, _: &Top, _, cx| {
                this.diff_scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Bottom, _, cx| {
                if let DiffState::Ready(l) = &this.diff {
                    let n = l.rows.rows.len();
                    if n > 0 {
                        this.diff_scroll.scroll_to_item_strict(n - 1, ScrollStrategy::Bottom);
                        cx.notify();
                    }
                }
            }))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if this.scroll_drag.is_some() {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        this.scroll_drag = None;
                        cx.notify();
                    } else {
                        this.drag_scroll_to(ev.position.y.into(), cx);
                    }
                    return;
                }
                if this.resizing {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        this.resizing = false;
                    } else {
                        let w: f32 = ev.position.x.into();
                        this.sidebar_w = w.clamp(theme::SIDEBAR_MIN, 900.);
                    }
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if this.scroll_drag.take().is_some() {
                        cx.notify();
                    }
                    if this.resizing {
                        this.resizing = false;
                        this.persisted.sidebar_w = Some(this.sidebar_w);
                        this.persisted.save();
                        cx.notify();
                    }
                }),
            )
            .child(self.render_titlebar(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .when(self.sidebar_open, |d| d.child(self.render_sidebar(window, cx)))
                    .when(self.sidebar_open, |d| {
                        d.child(
                            div()
                                .id("resize")
                                .w(px(4.))
                                .h_full()
                                .ml(px(-2.))
                                .mr(px(-2.))
                                .cursor_col_resize()
                                .hover(|s| s.bg(theme::line_hi()))
                                .when(self.resizing, |d| d.bg(theme::frost()))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.resizing = true;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        )
                    })
                    .child(self.render_diff_pane(window, cx)),
            )
            .child(self.render_status(cx))
            .children(self.render_picker(window, cx))
    }
}

impl Kerf {
    pub fn showing_collapsed_generated(&self) -> bool {
        match &self.diff {
            DiffState::Ready(l) => {
                l.target.change.is_generated() && !self.shown_generated.contains(&l.target.key())
            }
            _ => false,
        }
    }
}

#[derive(Default)]
struct Node<'a> {
    dirs: std::collections::BTreeMap<&'a str, Node<'a>>,
    files: Vec<usize>,
    /// Changed files anywhere below this directory.
    count: usize,
}

/// Directory tree rows for `visible` changes (indices into `changes`, sorted by path).
/// Folders come first; single-child folder chains merge into one row (`src/ui/`, like Zed);
/// children of collapsed folders are omitted.
pub fn tree_rows(changes: &[Change], visible: &[usize], collapsed: &HashSet<String>) -> Vec<ListRow> {
    let mut root = Node::default();
    for &i in visible {
        let parts: Vec<&str> = changes[i].path.split('/').collect();
        let mut node = &mut root;
        for dir in &parts[..parts.len() - 1] {
            node = node.dirs.entry(dir).or_default();
            node.count += 1;
        }
        node.files.push(i);
    }
    let mut rows = Vec::new();
    flatten(&root, "", 0, collapsed, &mut rows);
    rows
}

fn flatten(node: &Node<'_>, prefix: &str, depth: usize, collapsed: &HashSet<String>, rows: &mut Vec<ListRow>) {
    for (name, mut child) in node.dirs.iter().map(|(n, c)| (n.to_string(), c)) {
        let mut label = name;
        let mut path = if prefix.is_empty() { label.clone() } else { format!("{prefix}/{label}") };
        while child.files.is_empty() && child.dirs.len() == 1 {
            let (n2, c2) = child.dirs.iter().next().unwrap();
            label = format!("{label}/{n2}");
            path = format!("{path}/{n2}");
            child = c2;
        }
        let is_collapsed = collapsed.contains(&path);
        rows.push(ListRow::Dir { path: path.clone(), name: label, depth, collapsed: is_collapsed, files: child.count });
        if !is_collapsed {
            flatten(child, &path, depth + 1, collapsed, rows);
        }
    }
    rows.extend(node.files.iter().map(|&change| ListRow::File { change, depth }));
}

fn row_identity(r: &ListRow) -> String {
    match r {
        ListRow::Summary => "summary".into(),
        ListRow::Dir { path, .. } => format!("d:{path}"),
        ListRow::File { change, .. } => format!("f:{change}"),
        ListRow::Group { group, .. } => format!("g:{group:?}"),
        ListRow::Commit { group, idx } => format!("c:{group:?}:{idx}"),
        ListRow::CommitFile { change } => format!("cf:{change}"),
        ListRow::Notice(n) => format!("n:{n}"),
    }
}

/// Commits offered in the picker (newest across local branches).
const RECENT_COMMITS: usize = 300;
/// Commits listed in the picker when the query is empty.
const COMMITS_WHEN_EMPTY: usize = 30;

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn commit_label(c: &CommitInfo) -> String {
    format!("{} {}", c.short(), c.summary)
}

/// Builds picker entries: refs (grouped when the query is empty) then recent commits.
/// With a query, everything is ranked together; SHA-prefix hits on commits rank first.
/// A query that looks like a revision (hex ≥ 4, or contains `~ ^ @`) also offers `Raw`.
pub fn picker_items(query: &str, refs: &[RefInfo], commits: &[CommitInfo]) -> Vec<(PickItem, Vec<usize>)> {
    let mut out: Vec<(PickItem, i64, Vec<usize>)> = refs
        .iter()
        .enumerate()
        .filter_map(|(i, r)| fuzzy(query, &r.name).map(|(s, pos)| (PickItem::Ref(i), s, pos)))
        .collect();
    let q = query.to_lowercase();
    for (i, c) in commits.iter().enumerate() {
        if query.is_empty() {
            if i < COMMITS_WHEN_EMPTY {
                out.push((PickItem::Commit(i), i64::MIN, Vec::new()));
            }
            continue;
        }
        let sha = c.oid.to_string();
        if is_hex(&q) && q.len() >= 4 && sha.starts_with(&q) {
            out.push((PickItem::Commit(i), 10_000, (0..q.len().min(7)).collect()));
        } else if let Some((s, pos)) = fuzzy(query, &commit_label(c)) {
            // Commits rank below refs with the same score: branch names are the common case.
            out.push((PickItem::Commit(i), s - 50, pos));
        }
    }
    if !query.is_empty() {
        out.sort_by_key(|x| std::cmp::Reverse(x.1));
    }
    let revish = (is_hex(&q) && q.len() >= 4) || query.contains(['~', '^', '@']);
    let exact_sha = out.iter().any(|(it, s, _)| matches!(it, PickItem::Commit(_)) && *s == 10_000);
    if revish && !exact_sha {
        out.push((PickItem::Raw(query.to_string()), 0, Vec::new()));
    }
    out.into_iter().map(|(i, _, p)| (i, p)).collect()
}

fn digits(n: u32) -> usize {
    n.max(1).to_string().len().max(3)
}

fn widest(fd: &FileDiff, rows: &Rows) -> Option<usize> {
    let mut best = (0usize, None);
    for (i, row) in rows.rows.iter().enumerate() {
        let len = match row {
            diff::Row::Line { idx, .. } => fd.lines[*idx].range.len(),
            diff::Row::Pair { left, right } => {
                let l = left.as_ref().map(|c| fd.lines[c.idx].range.len()).unwrap_or(0);
                let r = right.as_ref().map(|c| fd.lines[c.idx].range.len()).unwrap_or(0);
                l.max(r)
            }
            _ => 0,
        };
        if len > best.0 {
            best = (len, Some(i));
        }
    }
    best.1
}

/// Subsequence fuzzy match. Returns (score, matched char positions). Empty query matches all.
pub fn fuzzy(query: &str, text: &str) -> Option<(i64, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let t: Vec<char> = text.chars().collect();
    let tl: Vec<char> = text.to_lowercase().chars().collect();
    if tl.len() != t.len() {
        return None;
    }
    let mut pos = Vec::with_capacity(q.len());
    let mut qi = 0;
    let mut score: i64 = 0;
    let mut last: Option<usize> = None;
    for (i, c) in tl.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            score += 10;
            if last == Some(i.wrapping_sub(1)) {
                score += 15; // consecutive
            }
            if i == 0 || matches!(t[i - 1], '/' | '-' | '_' | '.') {
                score += 20; // word start
            }
            pos.push(i);
            last = Some(i);
            qi += 1;
        }
    }
    if qi < q.len() {
        return None;
    }
    score -= t.len() as i64; // prefer shorter names
    Some((score, pos))
}

#[cfg(test)]
mod tests {
    use super::{fuzzy, tree_rows, ListRow};
    use crate::git::{Change, ChangeStatus};
    use std::collections::HashSet;

    fn ch(path: &str) -> Change {
        Change {
            status: ChangeStatus::Modified,
            path: path.into(),
            old_path: None,
            old_oid: None,
            new_oid: None,
            old_mode: 0o100644,
            new_mode: 0o100644,
            similarity: None,
            binary: false,
            additions: Some(1),
            deletions: Some(0),
        }
    }

    fn render(rows: &[ListRow], changes: &[Change]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                ListRow::Dir { name, depth, collapsed, .. } => {
                    format!("{}{}{}/", "  ".repeat(*depth), if *collapsed { "+" } else { "" }, name)
                }
                ListRow::File { change, depth } => {
                    format!("{}{}", "  ".repeat(*depth), changes[*change].path.rsplit('/').next().unwrap())
                }
                _ => "?".into(),
            })
            .collect()
    }

    #[test]
    fn tree_groups_by_directory() {
        let changes: Vec<Change> = ["README.md", "src/a/x.rs", "src/a/y.rs", "src/b.rs", "tests/t.rs"]
            .iter()
            .map(|p| ch(p))
            .collect();
        let visible: Vec<usize> = (0..changes.len()).collect();
        let rows = tree_rows(&changes, &visible, &HashSet::new());
        assert_eq!(
            render(&rows, &changes),
            ["src/", "  a/", "    x.rs", "    y.rs", "  b.rs", "tests/", "  t.rs", "README.md"]
        );
    }

    #[test]
    fn tree_same_name_dirs_under_different_parents() {
        let changes: Vec<Change> = ["a/lib/x.rs", "b/lib/y.rs"].iter().map(|p| ch(p)).collect();
        let rows = tree_rows(&changes, &[0, 1], &HashSet::new());
        assert_eq!(render(&rows, &changes), ["a/lib/", "  x.rs", "b/lib/", "  y.rs"]);
    }

    #[test]
    fn tree_compresses_single_child_chains_and_counts_files() {
        let changes: Vec<Change> = ["src/ui/app.rs", "src/ui/mod.rs", "src/git/mod.rs"].iter().map(|p| ch(p)).collect();
        let rows = tree_rows(&changes, &[0, 1, 2], &HashSet::new());
        assert_eq!(render(&rows, &changes), ["src/", "  git/", "    mod.rs", "  ui/", "    app.rs", "    mod.rs"]);
        let deep: Vec<Change> = ["a/b/c/d.rs"].iter().map(|p| ch(p)).collect();
        let rows = tree_rows(&deep, &[0], &HashSet::new());
        assert_eq!(render(&rows, &deep), ["a/b/c/", "  d.rs"]);
        match &rows[0] {
            ListRow::Dir { path, files, .. } => assert_eq!((path.as_str(), *files), ("a/b/c", 1)),
            _ => panic!(),
        }
    }

    #[test]
    fn tree_hides_children_of_collapsed_dir() {
        let changes: Vec<Change> = ["src/a/x.rs", "src/b.rs", "z.rs"].iter().map(|p| ch(p)).collect();
        let collapsed: HashSet<String> = ["src".to_string()].into();
        let rows = tree_rows(&changes, &[0, 1, 2], &collapsed);
        assert_eq!(render(&rows, &changes), ["+src/", "z.rs"]);
    }

    fn refs_and_commits() -> (Vec<crate::git::RefInfo>, Vec<crate::git::CommitInfo>) {
        use crate::git::{CommitInfo, RefInfo, RefKind};
        use git2::Oid;
        let oid = |h: &str| Oid::from_str(&format!("{h:0<40}")).unwrap();
        let r = |n: &str| RefInfo { name: n.into(), kind: RefKind::Local, target: oid("1"), summary: String::new(), time: 0, is_head: false };
        let c = |h: &str, s: &str| CommitInfo { oid: oid(h), summary: s.into(), message: s.into(), author: "a".into(), time: 0, parent_count: 1 };
        (
            vec![r("main"), r("feature/login")],
            vec![c("abc1234", "fix login bug"), c("def5678", "add readme")],
        )
    }

    #[test]
    fn picker_empty_query_lists_refs_then_commits() {
        use super::{picker_items, PickItem};
        let (refs, commits) = refs_and_commits();
        let items: Vec<PickItem> = picker_items("", &refs, &commits).into_iter().map(|(i, _)| i).collect();
        assert_eq!(items, [PickItem::Ref(0), PickItem::Ref(1), PickItem::Commit(0), PickItem::Commit(1)]);
    }

    #[test]
    fn picker_sha_prefix_ranks_commit_first() {
        use super::{picker_items, PickItem};
        let (refs, commits) = refs_and_commits();
        let items = picker_items("def5", &refs, &commits);
        assert_eq!(items[0].0, PickItem::Commit(1));
        assert!(!items.iter().any(|(i, _)| matches!(i, PickItem::Raw(_))));
    }

    #[test]
    fn picker_searches_commit_subjects_and_offers_raw_revisions() {
        use super::{picker_items, PickItem};
        let (refs, commits) = refs_and_commits();
        let items: Vec<PickItem> = picker_items("readme", &refs, &commits).into_iter().map(|(i, _)| i).collect();
        assert_eq!(items, [PickItem::Commit(1)]);
        let raw = picker_items("HEAD~3", &refs, &commits);
        assert_eq!(raw.last().unwrap().0, PickItem::Raw("HEAD~3".into()));
        let unknown_sha = picker_items("9999999", &refs, &commits);
        assert_eq!(unknown_sha.last().unwrap().0, PickItem::Raw("9999999".into()));
    }

    #[test]
    fn fuzzy_matches_subsequence_and_prefers_word_starts() {
        assert!(fuzzy("fl", "feature/login").is_some());
        assert!(fuzzy("xyz", "feature/login").is_none());
        let a = fuzzy("login", "feature/login").unwrap().0;
        let b = fuzzy("login", "feature/lxoxgxixn").unwrap().0;
        assert!(a > b);
    }
}
