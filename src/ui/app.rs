//! Root view: state, background jobs, and action handlers. Rendering lives in
//! sidebar.rs / diff_view.rs / picker.rs.

use super::state::Persisted;
use super::*;
use crate::diff::{self, Layout, Rows};
use crate::git::{
    short_sha, Change, ChangeStatus, CommitInfo, Comparison, DiffBody, DiffOptions, DiffSource, FileDiff,
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
use std::collections::{HashMap, HashSet};
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
    pub scratch: Option<Arc<Scratch>>,
}

impl Target {
    pub fn key(&self) -> String {
        if let Some(s) = &self.scratch {
            return format!("scratch:{}", s.id);
        }
        format!("{:?}:{:?}:{}", self.source.old, self.source.new, self.change.path)
    }

    /// A scratch target, i.e. a plain two-buffer diff outside git.
    pub fn scratch(scratch: Scratch) -> Self {
        let change = scratch.change();
        Target {
            source: DiffSource { old: None, new: Oid::ZERO_SHA1 },
            change,
            commit: None,
            scratch: Some(Arc::new(scratch)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// One side of a scratch diff: pasted text or a file's contents.
#[derive(Clone)]
pub struct ScratchSide {
    /// File path (display form) or "Pasted text".
    pub label: String,
    pub text: Arc<[u8]>,
}

impl ScratchSide {
    pub fn lines(&self) -> usize {
        let n = self.text.iter().filter(|&&b| b == b'\n').count();
        if self.text.last().is_some_and(|&b| b != b'\n') { n + 1 } else { n }
    }
    fn short_name(&self) -> String {
        self.label.rsplit('/').next().unwrap_or(&self.label).to_string()
    }
}

/// Plain diff of two texts (typed, pasted or files), independent of any repository.
/// `left` / `right` are the initial contents; while open, the live editors own the text.
#[derive(Clone)]
pub struct Scratch {
    pub id: u64,
    pub left: Option<ScratchSide>,
    pub right: Option<ScratchSide>,
}

impl Scratch {
    pub fn title(&self) -> String {
        match (&self.left, &self.right) {
            (Some(l), Some(r)) => {
                format!("{} ↔ {}", l.short_name(), r.short_name())
            }
            _ => "Untitled Diff".into(),
        }
    }
    fn change(&self) -> Change {
        Change {
            status: ChangeStatus::Modified,
            path: self.title(),
            old_path: None,
            old_oid: None,
            new_oid: None,
            old_mode: 0,
            new_mode: 0,
            similarity: None,
            binary: false,
            additions: None,
            deletions: None,
        }
    }
}

pub struct Loaded {
    pub target: Target,
    pub fd: Arc<FileDiff>,
    pub rows: Arc<Rows>,
    pub hl: Option<Arc<Vec<LineSpans>>>,
    /// Row with the longest text, used for horizontal sizing.
    pub widest_row: Option<usize>,
    /// Longest line on screen, in chars (sizes split halves for horizontal scroll).
    pub widest_chars: usize,
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

/// An open diff tab. The active tab's live state is `Kerf::diff`; others keep a cached copy.
pub struct DiffTab {
    pub target: Target,
    pub cached: Option<Arc<Loaded>>,
    pub scroll: UniformListScrollHandle,
    /// Preview tabs (italic) are replaced by the next file opened; pinned tabs stay.
    pub pinned: bool,
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
    pub tabs: Vec<DiffTab>,
    pub active_tab: Option<usize>,
    /// Set by double-click / Enter so the next opened file gets a pinned tab.
    pub pin_next: bool,
    pub info_open: bool,
    /// Comparison section expanded in the sidebar.
    pub range_open: bool,
    /// Soft-wrap long lines in the diff; layout cache keyed by (rows ptr, cols).
    pub wrap: bool,
    pub wrap_cache: Option<(usize, usize, Arc<crate::diff::Wrapped>)>,
    /// Logical row to scroll to once the next layout exists (after toggling wrap).
    pub pending_top_row: Option<usize>,
    /// Horizontal text offset in diffs (gutters stay fixed), its max, and bottom bar
    /// geometry `(track, thumb)`; active bar drag grab offset.
    pub hscroll: f32,
    pub hscroll_max: f32,
    pub hbar: (f32, f32),
    pub hbar_drag: Option<f32>,
    /// Split view divider position (0.2–0.8 of the width) and drag state.
    pub split_ratio: f32,
    pub split_drag: bool,
    /// On-screen bounds of the diff rows area.
    pub diff_area: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Commit message panel: expanded, max body height, active drag (start y, start height).
    pub banner_open: bool,
    pub banner_h: f32,
    pub banner_drag: Option<(f32, f32)>,
    pub shortcuts_open: bool,
    pub scratch_seq: u64,
    /// Editors per plain diff (scratch id).
    pub editors: HashMap<u64, super::scratch::EditorPair>,

    pub picker: Option<Picker>,
    pub sidebar_open: bool,
    pub sidebar_w: f32,
    pub resizing: bool,
    pub scroll_drag: Option<super::scrollbar::Drag>,
    pub flash: Option<(SharedString, Instant)>,
    flash_task: Option<Task<()>>,
}

impl Kerf {
    pub fn new(launch: super::Launch, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            mode: RangeMode::PrMerge,
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
            tabs: Vec::new(),
            active_tab: None,
            pin_next: false,
            info_open: false,
            range_open: true,
            wrap: false,
            wrap_cache: None,
            pending_top_row: None,
            hscroll: 0.,
            hscroll_max: 0.,
            hbar: (0., 0.),
            hbar_drag: None,
            split_ratio: 0.5,
            split_drag: false,
            diff_area: Default::default(),
            banner_open: true,
            banner_h: BANNER_DEFAULT_H,
            banner_drag: None,
            shortcuts_open: false,
            scratch_seq: 0,
            editors: HashMap::new(),
            picker: None,
            sidebar_open: true,
            resizing: false,
            scroll_drag: None,
            flash: None,
            flash_task: None,
        };
        this.range_open = !this.persisted.range_collapsed;
        this.wrap = this.persisted.wrap;
        this.split_ratio = this.persisted.split_ratio.unwrap_or(0.5).clamp(0.2, 0.8);
        this.banner_open = !this.persisted.banner_collapsed;
        this.banner_h = this.persisted.banner_h.unwrap_or(BANNER_DEFAULT_H);
        match launch {
            super::Launch::Files(a, b) => this.open_files_diff(a, b, cx),
            super::Launch::Repo(p) => this.open_repo(p, cx),
            super::Launch::Default => {
                if let Some(p) = this.persisted.last_repo.clone() {
                    this.open_repo(p, cx);
                }
            }
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
        self.tabs.clear();
        self.active_tab = None;
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
                this.rebind_tabs();
                this.expanded = None;
                this.rebuild_rows();
                // Keep the same file selected across refreshes when it still exists.
                let reselect = prev_target.and_then(|p| this.row_for_path(&p));
                let first = reselect.or_else(|| this.rows.iter().position(|r| matches!(r, ListRow::File { .. })));
                match first {
                    Some(ix) if this.tab == Tab::Files => this.select_row(ix, cx),
                    _ => match this.active_tab {
                        Some(a) => {
                            this.active_tab = None;
                            this.activate_tab(a, cx);
                        }
                        None => {
                            if this.tab == Tab::Files {
                                this.diff = DiffState::Empty;
                            }
                        }
                    },
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
            RangeMode::PrMerge => RangeMode::Compare,
            RangeMode::Compare => RangeMode::PrMerge,
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
                        Target { source: d.cmp.source, change: d.changes[change].clone(), commit: None, scratch: None },
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
                        self.show(Target { source: src, change: ch[change].clone(), commit, scratch: None }, cx);
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
            self.show(Target { source: src, change: first.clone(), commit, scratch: None }, cx);
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
        let pin = std::mem::take(&mut self.pin_next);
        self.open_target(target, pin, cx);
    }

    // ───────────────────────────── tabs ─────────────────────────────

    /// Opens `target`: focuses its tab if open, else replaces the preview tab or adds a new one.
    pub fn open_target(&mut self, target: Target, pin: bool, cx: &mut Context<Self>) {
        let key = target.key();
        if let Some(i) = self.tabs.iter().position(|t| t.target.key() == key) {
            if pin {
                self.tabs[i].pinned = true;
            }
            if self.active_tab != Some(i) {
                self.activate_tab(i, cx);
            }
            return;
        }
        self.stash_active();
        let tab = DiffTab { target: target.clone(), cached: None, scroll: UniformListScrollHandle::new(), pinned: pin };
        let pinned: Vec<bool> = self.tabs.iter().map(|t| t.pinned).collect();
        let (idx, replace) = place_tab(&pinned, self.active_tab);
        if replace {
            self.tabs[idx] = tab;
        } else {
            self.tabs.insert(idx, tab);
        }
        self.active_tab = Some(idx);
        self.diff_scroll = self.tabs[idx].scroll.clone();
        self.load_diff(target, true, cx);
    }

    /// Saves the active tab's ready diff so switching back is instant.
    fn stash_active(&mut self) {
        if let (Some(a), DiffState::Ready(l)) = (self.active_tab, &self.diff) {
            if let Some(t) = self.tabs.get_mut(a) {
                if t.target.key() == l.target.key() {
                    t.cached = Some(l.clone());
                }
            }
        }
    }

    pub fn activate_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        if i >= self.tabs.len() {
            return;
        }
        self.stash_active();
        self.active_tab = Some(i);
        self.diff_scroll = self.tabs[i].scroll.clone();
        match self.tabs[i].cached.clone() {
            Some(l) => {
                self.diff_gen += 1; // drop any in-flight load for the old tab
                self.hl_task = None;
                self.diff = DiffState::Ready(l);
            }
            None => {
                let t = self.tabs[i].target.clone();
                self.load_diff(t, false, cx);
            }
        }
        self.sync_selection_to_tab();
        cx.notify();
    }

    pub fn close_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        match self.active_tab {
            Some(a) if a == i => {
                if self.tabs.is_empty() {
                    self.active_tab = None;
                    self.diff_gen += 1;
                    self.diff = DiffState::Empty;
                } else {
                    // Zed/VS Code: activate the tab that slid into this slot, else the previous one.
                    self.active_tab = None;
                    self.activate_tab(i.min(self.tabs.len() - 1), cx);
                }
            }
            Some(a) if a > i => self.active_tab = Some(a - 1),
            _ => {}
        }
        cx.notify();
    }

    fn cycle_tab(&mut self, forward: bool, cx: &mut Context<Self>) {
        let n = self.tabs.len();
        let Some(a) = self.active_tab.filter(|_| n > 1) else { return };
        let next = if forward { (a + 1) % n } else { (a + n - 1) % n };
        self.activate_tab(next, cx);
    }

    pub fn pin_active(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.active_tab.and_then(|a| self.tabs.get_mut(a)) {
            t.pinned = true;
            cx.notify();
        }
    }

    /// After the range changes: point range-file tabs at the new range, drop ones whose file
    /// is no longer part of it. Commit tabs are independent of the range and stay.
    fn rebind_tabs(&mut self) {
        let Some(data) = self.range_data().cloned() else { return };
        let active_key = self.active_tab.and_then(|a| self.tabs.get(a)).map(|t| t.target.change.path.clone());
        self.tabs.retain_mut(|t| {
            if t.target.commit.is_some() || t.target.scratch.is_some() {
                return true;
            }
            match data.changes.iter().find(|c| c.path == t.target.change.path) {
                Some(c) => {
                    t.target = Target { source: data.cmp.source, change: c.clone(), commit: None, scratch: None };
                    t.cached = None;
                    true
                }
                None => false,
            }
        });
        self.active_tab = active_key
            .and_then(|p| self.tabs.iter().position(|t| t.target.commit.is_none() && t.target.change.path == p))
            .or_else(|| self.tabs.len().checked_sub(1));
        if let Some(a) = self.active_tab {
            self.diff_scroll = self.tabs[a].scroll.clone();
        }
    }

    /// Highlights the sidebar row for the active tab's file (Files tab only), without reopening.
    fn sync_selection_to_tab(&mut self) {
        let Some(t) = self.active_tab.and_then(|a| self.tabs.get(a)) else { return };
        if self.tab != Tab::Files || t.target.commit.is_some() || t.target.scratch.is_some() {
            return;
        }
        let path = t.target.change.path.clone();
        if let Some(ix) = self.row_for_path(&path) {
            self.selected = Some(ix);
            self.list_scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
    }

    /// Re-runs the active diff with current options; other tabs' caches are stale now.
    fn reload_diff(&mut self, cx: &mut Context<Self>) {
        for t in &mut self.tabs {
            t.cached = None;
        }
        if let Some(t) = self.current_target().cloned() {
            self.load_diff(t, false, cx);
        }
    }

    pub fn load_diff_public(&mut self, target: Target, cx: &mut Context<Self>) {
        self.load_diff(target, true, cx);
    }

    fn load_diff(&mut self, target: Target, reset_scroll: bool, cx: &mut Context<Self>) {
        let scratch = target.scratch.clone();
        if scratch.is_some() {
            // Plain diffs are live editors (see scratch.rs); nothing to load here.
            self.diff_gen += 1;
            self.diff = DiffState::Empty;
            cx.notify();
            return;
        }
        let path = self.repo_path.clone();
        if path.is_none() && scratch.is_none() {
            return;
        }
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
                let fd = match (&scratch, &path) {
                    (Some(s), _) => {
                        let (l, r) = (s.left.as_ref().unwrap(), s.right.as_ref().unwrap());
                        crate::git::diff_buffers(&l.text, &r.text, &l.label, &r.label, opts)?
                    }
                    (None, Some(path)) => Repo::open(path)?.file_diff(&change, opts)?,
                    (None, None) => anyhow::bail!("No repository"),
                };
                let rows = diff::build(&fd, layout);
                let widest_row = widest(&fd, &rows);
                let widest_chars = fd.lines.iter().map(|l| diff::display_chars(fd.line_text(l))).max().unwrap_or(0);
                let max_no = fd.lines.iter().filter_map(|l| l.old_no.max(l.new_no)).max().unwrap_or(1);
                anyhow::Ok((Arc::new(fd), Arc::new(rows), widest_row, widest_chars, digits(max_no)))
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
                    Ok((fd, rows, widest_row, widest_chars, gutter_digits)) => {
                        let loaded = Arc::new(Loaded { target: target.clone(), fd, rows, hl: None, widest_row, widest_chars, gutter_digits });
                        this.diff = DiffState::Ready(loaded.clone());
                        if reset_scroll {
                            this.hscroll = 0.;
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
                        widest_chars: l.widest_chars,
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
        if target.commit.is_none() && target.scratch.is_none() {
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
        self.recompute_all_live(cx);
        self.reload_diff(cx);
        cx.notify();
    }

    /// Active soft-wrap layout, if wrapping and it matches the loaded diff.
    fn wrapped(&self) -> Option<&Arc<crate::diff::Wrapped>> {
        let l = self.loaded()?;
        let (key, _, w) = self.wrap_cache.as_ref()?;
        (self.wrap && *key == Arc::as_ptr(&l.rows) as usize).then_some(w)
    }

    /// Rows the diff list actually draws (visual rows when wrapping).
    pub fn visual_len(&self) -> usize {
        match self.wrapped() {
            Some(w) => w.map.len(),
            None => self.loaded().map(|l| l.rows.rows.len()).unwrap_or(0),
        }
    }

    fn to_visual(&self, logical: usize) -> usize {
        self.wrapped().map(|w| w.starts[logical.min(w.starts.len() - 1)]).unwrap_or(logical)
    }

    fn to_logical(&self, visual: usize) -> usize {
        self.wrapped().and_then(|w| w.map.get(visual).map(|m| m.0 as usize)).unwrap_or(visual)
    }

    /// Top visible visual row.
    fn top_visual(&self) -> usize {
        let offset = -self.diff_scroll.0.borrow().base_handle.offset().y;
        (offset / theme::ROW_CODE).floor().max(0.) as usize
    }

    pub fn toggle_wrap(&mut self, cx: &mut Context<Self>) {
        // Keep the same logical row at the top across the switch.
        let top = self.to_logical(self.top_visual());
        self.wrap = !self.wrap;
        self.persisted.wrap = self.wrap;
        self.persisted.save();
        self.wrap_cache = None;
        self.pending_top_row = Some(top);
        cx.notify();
    }

    fn jump_hunk(&mut self, forward: bool, cx: &mut Context<Self>) {
        let DiffState::Ready(l) = &self.diff else { return };
        let current = self.to_logical(self.top_visual());
        let target = if forward {
            l.rows.hunk_rows.iter().copied().find(|&r| r > current)
        } else {
            l.rows.hunk_rows.iter().rev().copied().find(|&r| r + 1 < current.max(1))
        };
        if let Some(r) = target {
            let v = self.to_visual(r);
            self.diff_scroll.scroll_to_item_strict(v, ScrollStrategy::Top);
            cx.notify();
        } else if forward {
            self.step_file(true, cx);
        }
    }

    fn page(&mut self, rows: isize, cx: &mut Context<Self>) {
        let len = self.visual_len();
        if len == 0 {
            return;
        }
        let current = self.top_visual() as isize;
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
        if self.info_open || self.shortcuts_open {
            self.info_open = false;
            self.shortcuts_open = false;
            cx.notify();
            return;
        }
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
        let editing_text = self.editors.values().any(|p| p.left.read(cx).is_focused(window) || p.right.read(cx).is_focused(window));
        let context = if self.input == Input::None && !editing_text { "Kerf" } else { "KerfInput" };
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
                    } else if matches!(
                        this.selected.and_then(|i| this.rows.get(i)),
                        Some(ListRow::File { .. }) | Some(ListRow::CommitFile { .. })
                    ) {
                        this.pin_active(cx);
                    } else {
                        this.toggle_row(None, cx);
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &Cancel, _, cx| this.close_input(cx)))
            .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
                if let Some(a) = this.active_tab {
                    this.close_tab(a, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| this.cycle_tab(true, cx)))
            .on_action(cx.listener(|this, _: &PrevTab, _, cx| this.cycle_tab(false, cx)))
            .on_action(cx.listener(|this, _: &NewDiff, _, cx| this.new_scratch(cx)))
            .on_action(cx.listener(|this, _: &CompareFiles, _, cx| this.prompt_compare_files(cx)))
            .on_action(cx.listener(|this, _: &Paste, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &ShowShortcuts, _, cx| {
                this.shortcuts_open = !this.shortcuts_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowInfo, _, cx| {
                this.info_open = !this.info_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextFile, _, cx| this.step_file(true, cx)))
            .on_action(cx.listener(|this, _: &PrevFile, _, cx| this.step_file(false, cx)))
            .on_action(cx.listener(|this, _: &NextHunk, _, cx| this.jump_hunk(true, cx)))
            .on_action(cx.listener(|this, _: &PrevHunk, _, cx| this.jump_hunk(false, cx)))
            .on_action(cx.listener(|this, _: &ToggleSplit, _, cx| {
                let next = if this.layout == Layout::Split { Layout::Unified } else { Layout::Split };
                this.set_layout(next, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleWhitespace, _, cx| this.toggle_ws(cx)))
            .on_action(cx.listener(|this, _: &ToggleWrap, _, cx| this.toggle_wrap(cx)))
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
                if matches!(this.diff, DiffState::Ready(_)) {
                    let n = this.visual_len();
                    if n > 0 {
                        this.diff_scroll.scroll_to_item_strict(n - 1, ScrollStrategy::Bottom);
                        cx.notify();
                    }
                }
            }))
            // Clicking anywhere ends search mode; the search field's own click re-enters it.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.input == Input::Filter {
                        this.input = Input::None;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, window, cx| {
                if this.split_drag || this.hbar_drag.is_some() {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        this.split_drag = false;
                        this.hbar_drag = None;
                    } else if this.split_drag {
                        this.drag_split_to(ev.position.x.into(), cx);
                    } else {
                        this.drag_hbar_to(ev.position.x.into(), cx);
                    }
                    cx.notify();
                    return;
                }
                if let Some((start_y, start_h)) = this.banner_drag {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        this.banner_drag = None;
                    } else {
                        let y: f32 = ev.position.y.into();
                        let max = f32::from(window.viewport_size().height) * 0.6;
                        this.banner_h = (start_h + y - start_y).clamp(BANNER_MIN_H, max.max(BANNER_MIN_H));
                    }
                    cx.notify();
                    return;
                }
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
                    if std::mem::take(&mut this.split_drag) {
                        this.persisted.split_ratio = Some(this.split_ratio);
                        this.persisted.save();
                        cx.notify();
                    }
                    if this.hbar_drag.take().is_some() {
                        cx.notify();
                    }
                    if this.banner_drag.take().is_some() {
                        this.persisted.banner_h = Some(this.banner_h);
                        this.persisted.save();
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
                    .when(self.sidebar_open && self.repo_path.is_some(), |d| d.child(self.render_sidebar(window, cx)))
                    .when(self.sidebar_open && self.repo_path.is_some(), |d| {
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
            .children(self.render_info(window, cx))
            .children(self.render_shortcuts(window, cx))
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

/// Where a newly opened file goes: `(index, replace?)`. An unpinned (preview) active tab is
/// replaced; otherwise the new tab is inserted right after the active one (or appended).
pub fn place_tab(pinned: &[bool], active: Option<usize>) -> (usize, bool) {
    match active {
        Some(a) if a < pinned.len() && !pinned[a] => (a, true),
        Some(a) if a < pinned.len() => (a + 1, false),
        _ => (pinned.len(), false),
    }
}

/// Commit message panel body height: default and minimum (px).
const BANNER_DEFAULT_H: f32 = 140.;
const BANNER_MIN_H: f32 = 40.;

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
    fn preview_tab_is_replaced_pinned_tab_is_kept() {
        use super::place_tab;
        assert_eq!(place_tab(&[], None), (0, false)); // first tab
        assert_eq!(place_tab(&[false], Some(0)), (0, true)); // replace preview
        assert_eq!(place_tab(&[true], Some(0)), (1, false)); // after pinned
        assert_eq!(place_tab(&[true, true, true], Some(1)), (2, false)); // right after active
        assert_eq!(place_tab(&[true, false], Some(1)), (1, true));
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
