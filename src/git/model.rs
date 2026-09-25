//! Plain data types produced by the git layer. UI-free and `Send`.

use git2::Oid;
use std::ops::Range;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone)]
pub struct RefInfo {
    /// Short name as the user types it: `main`, `origin/main`, `v1.0`.
    pub name: String,
    pub kind: RefKind,
    pub target: Oid,
    pub summary: String,
    /// Commit time, seconds since epoch.
    pub time: i64,
    pub is_head: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RangeMode {
    /// `merge-base(base, compare)` → `compare`: what Compare introduced.
    #[default]
    ThreeDot,
    /// `base` → `compare`, tip to tip.
    TwoDot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeSpec {
    pub base: String,
    pub compare: String,
    pub mode: RangeMode,
}

/// Two trees to diff. `old` is `None` for a root commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffSource {
    pub old: Option<Oid>,
    pub new: Oid,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub oid: Oid,
    pub summary: String,
    pub message: String,
    pub author: String,
    pub time: i64,
    pub parent_count: usize,
}

impl CommitInfo {
    pub fn short(&self) -> String {
        short_sha(self.oid)
    }
    pub fn is_merge(&self) -> bool {
        self.parent_count > 1
    }
}

pub fn short_sha(oid: Oid) -> String {
    oid.to_string()[..7].to_string()
}

#[derive(Debug, Clone)]
pub struct Comparison {
    pub base: Oid,
    pub compare: Oid,
    pub merge_base: Option<Oid>,
    /// Mode actually used: falls back to two-dot when there is no merge base.
    pub mode: RangeMode,
    /// Commits in compare, not in base (newest first).
    pub ahead: Vec<CommitInfo>,
    /// Commits in base, not in compare (newest first).
    pub behind: Vec<CommitInfo>,
    pub source: DiffSource,
}

impl Comparison {
    pub fn unrelated(&self) -> bool {
        self.merge_base.is_none()
    }
    pub fn identical(&self) -> bool {
        self.base == self.compare
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChange,
}

impl ChangeStatus {
    pub fn glyph(self) -> &'static str {
        match self {
            ChangeStatus::Added => "A",
            ChangeStatus::Modified => "M",
            ChangeStatus::Deleted => "D",
            ChangeStatus::Renamed => "R",
            ChangeStatus::Copied => "C",
            ChangeStatus::TypeChange => "T",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Change {
    pub status: ChangeStatus,
    /// Path shown to the user (new path, or old path for deletions).
    pub path: String,
    /// Previous path for renames/copies.
    pub old_path: Option<String>,
    pub old_oid: Option<Oid>,
    pub new_oid: Option<Oid>,
    pub old_mode: u32,
    pub new_mode: u32,
    pub similarity: Option<u16>,
    pub binary: bool,
    /// `None` when stats were skipped (binary or over the stats size limit).
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

impl Change {
    pub fn is_submodule(&self) -> bool {
        self.old_mode == 0o160000 || self.new_mode == 0o160000
    }
    pub fn mode_changed(&self) -> bool {
        self.old_mode != 0 && self.new_mode != 0 && self.old_mode != self.new_mode
    }
    pub fn is_generated(&self) -> bool {
        is_generated_path(&self.path)
    }
}

/// Lockfiles, minified bundles and build output start collapsed.
pub fn is_generated_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    const LOCKS: &[&str] = &[
        "Cargo.lock",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
        "Gemfile.lock",
        "poetry.lock",
        "composer.lock",
        "go.sum",
        "Podfile.lock",
        "uv.lock",
    ];
    LOCKS.contains(&name)
        || name.contains(".min.")
        || name.ends_with(".map")
        || path.starts_with("dist/")
        || path.contains("/dist/")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    /// Byte range into `FileDiff::text` (no trailing newline).
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// Text after the `@@ … @@` marker (enclosing function), trimmed.
    pub context: String,
    /// Index of the first line of this hunk in `FileDiff::lines`.
    pub first_line: usize,
    pub line_count: usize,
}

#[derive(Debug, Clone)]
pub enum DiffBody {
    /// Normal text diff.
    Text,
    Binary,
    /// Over the size limit and not forced.
    TooLarge,
    /// Submodule pointer moved.
    Submodule { old: Option<Oid>, new: Option<Oid> },
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub body: DiffBody,
    pub old_size: u64,
    pub new_size: u64,
    /// Line count of the new side (or old side for deletions); for large-file gate display.
    pub total_lines: usize,
    /// One shared buffer; every `DiffLine` points into it.
    pub text: Arc<str>,
    pub lines: Vec<DiffLine>,
    pub hunks: Vec<Hunk>,
    pub non_utf8: bool,
    pub old_no_newline: bool,
    pub new_no_newline: bool,
}

impl FileDiff {
    pub fn line_text(&self, line: &DiffLine) -> &str {
        &self.text[line.range.clone()]
    }
    pub fn additions(&self) -> usize {
        self.lines.iter().filter(|l| l.kind == LineKind::Added).count()
    }
    pub fn deletions(&self) -> usize {
        self.lines.iter().filter(|l| l.kind == LineKind::Removed).count()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DiffOptions {
    pub ignore_whitespace: bool,
    pub context_lines: u32,
    /// Blobs larger than this (either side) produce `DiffBody::TooLarge` unless `force`.
    pub max_bytes: u64,
    pub force: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            ignore_whitespace: false,
            context_lines: 3,
            max_bytes: 20 * 1024 * 1024,
            force: false,
        }
    }
}
