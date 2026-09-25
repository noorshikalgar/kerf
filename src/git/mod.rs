//! Read-only git facade. Everything the UI needs from a repository goes through `Repo`.
//! Nothing here writes refs, the index or the working tree.

pub mod model;

pub use model::*;

use anyhow::{anyhow, bail, Context as _, Result};
use git2::{BranchType, Delta, DiffFindOptions, ObjectType, Oid, Patch, Repository, Sort};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Max commits listed per direction; protects the UI from pathological histories.
pub const MAX_COMMITS: usize = 20_000;
/// Per-file blob size above which +/- stats are skipped in the change list.
pub const STATS_MAX_BYTES: u64 = 4 * 1024 * 1024;

pub struct Repo {
    repo: Repository,
    root: PathBuf,
}

impl Repo {
    /// Opens the repository containing `path` (walks up to find the root).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let repo = Repository::discover(path)
            .map_err(|_| anyhow!("Not a Git repository: {}", path.display()))?;
        let root = repo
            .workdir()
            .unwrap_or_else(|| repo.path())
            .to_path_buf();
        let root = root.canonicalize().unwrap_or(root);
        Ok(Self { repo, root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().trim_end_matches(".git").to_string())
            .unwrap_or_else(|| self.root.display().to_string())
    }

    pub fn is_empty(&self) -> bool {
        self.repo.is_empty().unwrap_or(true)
    }

    /// Short name of the checked-out branch, or `None` when detached/unborn.
    pub fn head_branch(&self) -> Option<String> {
        let head = self.repo.head().ok()?;
        if head.is_branch() {
            head.shorthand().ok().map(str::to_string)
        } else {
            None
        }
    }

    /// Local branches, remote-tracking branches and tags; newest first within each kind.
    pub fn refs(&self) -> Result<Vec<RefInfo>> {
        let head = self.head_branch();
        let mut out = Vec::new();
        for (bt, kind) in [(BranchType::Local, RefKind::Local), (BranchType::Remote, RefKind::Remote)] {
            for branch in self.repo.branches(Some(bt))? {
                let (branch, _) = branch?;
                let Some(name) = branch.name()?.map(str::to_string) else { continue };
                if kind == RefKind::Remote && name.ends_with("/HEAD") {
                    continue;
                }
                let Ok(commit) = branch.get().peel_to_commit() else { continue };
                out.push(RefInfo {
                    is_head: kind == RefKind::Local && head.as_deref() == Some(name.as_str()),
                    name,
                    kind,
                    target: commit.id(),
                    summary: summary_of(&commit),
                    time: commit.time().seconds(),
                });
            }
        }
        self.repo.tag_foreach(|oid, full| {
            let full = String::from_utf8_lossy(full);
            let name = full.trim_start_matches("refs/tags/").to_string();
            if let Ok(commit) = self
                .repo
                .find_object(oid, None)
                .and_then(|o| o.peel_to_commit())
            {
                out.push(RefInfo {
                    name,
                    kind: RefKind::Tag,
                    target: commit.id(),
                    summary: summary_of(&commit),
                    time: commit.time().seconds(),
                    is_head: false,
                });
            }
            true
        })?;
        out.sort_by(|a, b| {
            kind_rank(a.kind)
                .cmp(&kind_rank(b.kind))
                .then(b.time.cmp(&a.time))
                .then(a.name.cmp(&b.name))
        });
        Ok(out)
    }

    /// Picks sensible defaults: base = main/master/develop/trunk (not HEAD when possible),
    /// compare = current branch (or the newest other local branch).
    pub fn default_range(&self, refs: &[RefInfo]) -> Option<(String, String)> {
        let locals: Vec<&RefInfo> = refs.iter().filter(|r| r.kind == RefKind::Local).collect();
        if locals.is_empty() {
            return None;
        }
        let head = self.head_branch();
        let preferred = ["main", "master", "develop", "trunk"];
        let base = preferred
            .iter()
            .find(|p| locals.iter().any(|r| r.name == **p) && head.as_deref() != Some(**p))
            .map(|s| s.to_string())
            .or_else(|| preferred.iter().find(|p| locals.iter().any(|r| r.name == **p)).map(|s| s.to_string()))
            .unwrap_or_else(|| locals[0].name.clone());
        let compare = head
            .filter(|h| *h != base)
            .or_else(|| locals.iter().find(|r| r.name != base).map(|r| r.name.clone()))
            .unwrap_or_else(|| base.clone());
        Some((base, compare))
    }

    /// Resolves a branch, tag, remote branch or SHA to a commit id.
    pub fn resolve(&self, name: &str) -> Result<Oid> {
        let obj = self
            .repo
            .revparse_single(name)
            .with_context(|| format!("Unknown ref: {name}"))?;
        Ok(obj.peel_to_commit().with_context(|| format!("Not a commit: {name}"))?.id())
    }

    pub fn compare(&self, spec: &RangeSpec) -> Result<Comparison> {
        let base = self.resolve(&spec.base)?;
        let compare = self.resolve(&spec.compare)?;
        let merge_base = self.repo.merge_base(base, compare).ok();
        let mode = if merge_base.is_none() { RangeMode::TwoDot } else { spec.mode };
        let from_commit = match mode {
            RangeMode::ThreeDot => merge_base.unwrap_or(base),
            RangeMode::TwoDot => base,
        };
        let source = DiffSource {
            old: Some(self.repo.find_commit(from_commit)?.tree_id()),
            new: self.repo.find_commit(compare)?.tree_id(),
        };
        Ok(Comparison {
            base,
            compare,
            merge_base,
            mode,
            ahead: self.commits_between(compare, base)?,
            behind: self.commits_between(base, compare)?,
            source,
        })
    }

    /// Commits reachable from `include` but not from `exclude`, newest first.
    fn commits_between(&self, include: Oid, exclude: Oid) -> Result<Vec<CommitInfo>> {
        if include == exclude {
            return Ok(Vec::new());
        }
        let mut walk = self.repo.revwalk()?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        walk.push(include)?;
        walk.hide(exclude)?;
        let mut out = Vec::new();
        for oid in walk.take(MAX_COMMITS) {
            out.push(self.commit_info(oid?)?);
        }
        Ok(out)
    }

    pub fn commit_info(&self, oid: Oid) -> Result<CommitInfo> {
        let c = self.repo.find_commit(oid)?;
        let author = c.author();
        Ok(CommitInfo {
            oid,
            summary: summary_of(&c),
            message: String::from_utf8_lossy(c.message_bytes()).trim_end().to_string(),
            author: author.name().unwrap_or("").to_string(),
            time: c.time().seconds(),
            parent_count: c.parent_count(),
        })
    }

    /// A commit's own changes: first parent → commit (empty tree for root commits).
    pub fn commit_source(&self, oid: Oid) -> Result<DiffSource> {
        let c = self.repo.find_commit(oid)?;
        let old = if c.parent_count() > 0 { Some(c.parent(0)?.tree_id()) } else { None };
        Ok(DiffSource { old, new: c.tree_id() })
    }

    /// Changed files between two trees, with rename detection and per-file stats.
    pub fn changes(&self, src: DiffSource) -> Result<Vec<Change>> {
        let old_tree = src.old.map(|o| self.repo.find_tree(o)).transpose()?;
        let new_tree = self.repo.find_tree(src.new)?;
        let mut opts = git2::DiffOptions::new();
        opts.context_lines(0).include_typechange(true);
        let mut diff = self
            .repo
            .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;
        let mut find = DiffFindOptions::new();
        find.renames(true).copies(false).rename_limit(2000);
        diff.find_similar(Some(&mut find))?;

        let mut out = Vec::with_capacity(diff.deltas().len());
        for (idx, delta) in diff.deltas().enumerate() {
            let status = match delta.status() {
                Delta::Added => ChangeStatus::Added,
                Delta::Deleted => ChangeStatus::Deleted,
                Delta::Modified => ChangeStatus::Modified,
                Delta::Renamed => ChangeStatus::Renamed,
                Delta::Copied => ChangeStatus::Copied,
                Delta::Typechange => ChangeStatus::TypeChange,
                _ => continue,
            };
            let old_path = delta.old_file().path().map(|p| p.to_string_lossy().to_string());
            let new_path = delta.new_file().path().map(|p| p.to_string_lossy().to_string());
            let path = match status {
                ChangeStatus::Deleted => old_path.clone(),
                _ => new_path.clone(),
            }
            .unwrap_or_default();
            let old_oid = nonzero(delta.old_file().id());
            let new_oid = nonzero(delta.new_file().id());
            let old_mode = u32::from(delta.old_file().mode());
            let new_mode = u32::from(delta.new_file().mode());
            let is_sub = old_mode == 0o160000 || new_mode == 0o160000;

            let old_size = old_oid.filter(|_| !is_sub).map(|o| self.blob_size(o)).unwrap_or(0);
            let new_size = new_oid.filter(|_| !is_sub).map(|o| self.blob_size(o)).unwrap_or(0);
            let mut binary = delta.flags().is_binary();
            let (mut additions, mut deletions) = (None, None);
            if !is_sub && old_size.max(new_size) <= STATS_MAX_BYTES {
                if let Ok(Some(patch)) = Patch::from_diff(&diff, idx) {
                    binary |= patch.delta().flags().is_binary();
                    if !binary {
                        if let Ok((_, a, d)) = patch.line_stats() {
                            additions = Some(a as u32);
                            deletions = Some(d as u32);
                        }
                    }
                }
            }

            out.push(Change {
                status,
                old_path: matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied)
                    .then_some(old_path)
                    .flatten(),
                path,
                old_oid,
                new_oid,
                old_mode,
                new_mode,
                similarity: matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied)
                    .then(|| similarity_of(&delta)),
                binary,
                additions,
                deletions,
            });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn blob_size(&self, oid: Oid) -> u64 {
        self.repo
            .odb()
            .and_then(|odb| odb.read_header(oid))
            .map(|(size, _)| size as u64)
            .unwrap_or(0)
    }

    /// Full line-level diff of one change.
    pub fn file_diff(&self, change: &Change, opts: DiffOptions) -> Result<FileDiff> {
        let old_size = change.old_oid.filter(|_| !change.is_submodule()).map(|o| self.blob_size(o)).unwrap_or(0);
        let new_size = change.new_oid.filter(|_| !change.is_submodule()).map(|o| self.blob_size(o)).unwrap_or(0);
        let mut fd = FileDiff {
            path: change.path.clone(),
            body: DiffBody::Text,
            old_size,
            new_size,
            total_lines: 0,
            text: Arc::from(""),
            lines: Vec::new(),
            hunks: Vec::new(),
            non_utf8: false,
            old_no_newline: false,
            new_no_newline: false,
        };
        if change.is_submodule() {
            fd.body = DiffBody::Submodule { old: change.old_oid, new: change.new_oid };
            return Ok(fd);
        }
        if !opts.force && old_size.max(new_size) > opts.max_bytes {
            fd.body = DiffBody::TooLarge;
            return Ok(fd);
        }

        let old_blob = change.old_oid.map(|o| self.repo.find_blob(o)).transpose()?;
        let new_blob = change.new_oid.map(|o| self.repo.find_blob(o)).transpose()?;
        let old_bytes: &[u8] = old_blob.as_ref().map(|b| b.content()).unwrap_or(&[]);
        let new_bytes: &[u8] = new_blob.as_ref().map(|b| b.content()).unwrap_or(&[]);
        if change.binary
            || old_blob.as_ref().is_some_and(|b| b.is_binary())
            || new_blob.as_ref().is_some_and(|b| b.is_binary())
        {
            fd.body = DiffBody::Binary;
            return Ok(fd);
        }
        fd.non_utf8 = std::str::from_utf8(old_bytes).is_err() || std::str::from_utf8(new_bytes).is_err();
        fd.total_lines = bytecount_lines(if new_bytes.is_empty() { old_bytes } else { new_bytes });

        let mut gopts = git2::DiffOptions::new();
        gopts
            .context_lines(opts.context_lines)
            .ignore_whitespace(opts.ignore_whitespace)
            .force_text(true);
        let old_path = change.old_path.as_deref().unwrap_or(&change.path);
        let patch = Patch::from_buffers(
            old_bytes,
            Some(Path::new(old_path)),
            new_bytes,
            Some(Path::new(&change.path)),
            Some(&mut gopts),
        )?;

        let mut text = String::with_capacity(old_bytes.len().max(new_bytes.len()));
        for h in 0..patch.num_hunks() {
            let (hunk, n) = patch.hunk(h)?;
            let first_line = fd.lines.len();
            for l in 0..n {
                let line = patch.line_in_hunk(h, l)?;
                let kind = match line.origin() {
                    ' ' => LineKind::Context,
                    '+' => LineKind::Added,
                    '-' => LineKind::Removed,
                    // "\ No newline at end of file" markers (libgit2 EOFNL origins):
                    // '>' old lacks a final LF, '<' new lacks it, '=' both lack it.
                    '>' => {
                        fd.old_no_newline = true;
                        continue;
                    }
                    '<' => {
                        fd.new_no_newline = true;
                        continue;
                    }
                    '=' => {
                        fd.old_no_newline = true;
                        fd.new_no_newline = true;
                        continue;
                    }
                    _ => continue,
                };
                let content = String::from_utf8_lossy(line.content());
                let content = content.trim_end_matches('\n').trim_end_matches('\r');
                let start = text.len();
                text.push_str(content);
                fd.lines.push(DiffLine {
                    kind,
                    old_no: line.old_lineno(),
                    new_no: line.new_lineno(),
                    range: start..text.len(),
                });
            }
            let header = String::from_utf8_lossy(hunk.header()).to_string();
            fd.hunks.push(Hunk {
                old_start: hunk.old_start(),
                old_lines: hunk.old_lines(),
                new_start: hunk.new_start(),
                new_lines: hunk.new_lines(),
                context: hunk_context(&header),
                first_line,
                line_count: fd.lines.len() - first_line,
            });
        }
        fd.text = Arc::from(text);
        Ok(fd)
    }

    /// Changes plus diff source for a single commit.
    pub fn commit_changes(&self, oid: Oid) -> Result<(DiffSource, Vec<Change>)> {
        let src = self.commit_source(oid)?;
        Ok((src, self.changes(src)?))
    }

    pub fn object_kind(&self, oid: Oid) -> Option<ObjectType> {
        self.repo.find_object(oid, None).ok().and_then(|o| o.kind())
    }
}

fn summary_of(c: &git2::Commit<'_>) -> String {
    c.summary_bytes()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

fn kind_rank(k: RefKind) -> u8 {
    match k {
        RefKind::Local => 0,
        RefKind::Remote => 1,
        RefKind::Tag => 2,
    }
}

fn nonzero(oid: Oid) -> Option<Oid> {
    (!oid.is_zero()).then_some(oid)
}

fn similarity_of(delta: &git2::DiffDelta<'_>) -> u16 {
    // libgit2 exposes similarity only via the raw struct.
    use git2::Binding;
    unsafe { (*delta.raw()).similarity }
}

fn hunk_context(header: &str) -> String {
    // "@@ -1,3 +1,4 @@ fn main() {\n" → "fn main() {"
    header
        .trim_end()
        .splitn(3, "@@")
        .nth(2)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn bytecount_lines(b: &[u8]) -> usize {
    if b.is_empty() {
        return 0;
    }
    let n = b.iter().filter(|&&c| c == b'\n').count();
    if b.last() == Some(&b'\n') { n } else { n + 1 }
}

/// Validates both refs exist before a comparison is attempted; returns a user-facing error.
pub fn validate_spec(repo: &Repo, spec: &RangeSpec) -> Result<()> {
    if spec.base.is_empty() || spec.compare.is_empty() {
        bail!("Pick both a base and a compare branch");
    }
    repo.resolve(&spec.base)?;
    repo.resolve(&spec.compare)?;
    Ok(())
}

#[cfg(test)]
mod tests;
