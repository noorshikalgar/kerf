//! Behaviour tests for the git facade. Each test builds a real repository with the git CLI.

use super::*;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let f = Self { dir: TempDir::new().unwrap() };
        f.git(&["init", "-q", "-b", "main"]);
        f
    }
    fn path(&self) -> &Path {
        self.dir.path()
    }
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(self.path())
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
    fn write(&self, path: &str, content: &str) {
        let p = self.path().join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }
    fn commit(&self, msg: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", msg]);
    }
    fn repo(&self) -> Repo {
        Repo::open(self.path()).unwrap()
    }
}

fn spec(base: &str, compare: &str) -> RangeSpec {
    RangeSpec { base: base.into(), compare: compare.into(), mode: RangeMode::PrMerge }
}

/// main: A ─ B(main)        feature: A ─ C ─ D(feature)
fn diverged() -> Fixture {
    let f = Fixture::new();
    f.write("README.md", "hello\n");
    f.write("src/lib.rs", "fn a() {}\nfn b() {}\nfn c() {}\n");
    f.commit("A: init");
    f.git(&["checkout", "-q", "-b", "feature"]);
    f.write("src/lib.rs", "fn a() {}\nfn b2() {}\nfn c() {}\n");
    f.commit("C: rename b");
    f.write("src/new.rs", "pub fn n() {}\n");
    f.commit("D: add new");
    f.git(&["checkout", "-q", "main"]);
    f.write("README.md", "hello main\n");
    f.commit("B: main readme");
    f.git(&["checkout", "-q", "feature"]);
    f
}

#[test]
fn open_rejects_non_repo() {
    let dir = TempDir::new().unwrap();
    let err = Repo::open(dir.path()).err().unwrap().to_string();
    assert!(err.contains("Not a Git repository"), "{err}");
}

#[test]
fn open_discovers_root_from_subdir() {
    let f = diverged();
    let r = Repo::open(f.path().join("src")).unwrap();
    assert_eq!(r.root(), f.path().canonicalize().unwrap());
}

#[test]
fn empty_repo_is_detected() {
    let f = Fixture::new();
    let r = f.repo();
    assert!(r.is_empty());
    assert!(r.refs().unwrap().is_empty());
    assert!(r.default_range(&[]).is_none());
}

#[test]
fn refs_lists_branches_and_tags_with_head() {
    let f = diverged();
    f.git(&["tag", "v1", "main"]);
    let refs = f.repo().refs().unwrap();
    let names: Vec<_> = refs.iter().map(|r| (r.name.as_str(), r.kind)).collect();
    assert!(names.contains(&("main", RefKind::Local)));
    assert!(names.contains(&("feature", RefKind::Local)));
    assert!(names.contains(&("v1", RefKind::Tag)));
    assert!(refs.iter().find(|r| r.name == "feature").unwrap().is_head);
    // locals sort before tags
    assert_eq!(refs.last().unwrap().kind, RefKind::Tag);
}

#[test]
fn default_range_is_main_vs_current() {
    let f = diverged();
    let r = f.repo();
    let refs = r.refs().unwrap();
    assert_eq!(r.default_range(&refs), Some(("main".into(), "feature".into())));
}

#[test]
fn three_dot_shows_only_compare_changes() {
    let f = diverged();
    let r = f.repo();
    let cmp = r.compare(&spec("main", "feature")).unwrap();
    assert_eq!(cmp.mode, RangeMode::PrMerge);
    assert!(cmp.merge_base.is_some());
    assert_eq!(cmp.ahead.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["D: add new", "C: rename b"]);
    assert_eq!(cmp.behind.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["B: main readme"]);

    let changes = r.changes(cmp.source).unwrap();
    let paths: Vec<_> = changes.iter().map(|c| (c.path.as_str(), c.status)).collect();
    // README change on main is NOT part of a three-dot diff.
    assert_eq!(paths, [("src/lib.rs", ChangeStatus::Modified), ("src/new.rs", ChangeStatus::Added)]);
    let lib = &changes[0];
    assert_eq!((lib.additions, lib.deletions), (Some(1), Some(1)));
}

#[test]
fn two_dot_includes_base_side_changes() {
    let f = diverged();
    let r = f.repo();
    let cmp = r
        .compare(&RangeSpec { mode: RangeMode::Compare, ..spec("main", "feature") })
        .unwrap();
    let paths: Vec<_> = r.changes(cmp.source).unwrap().into_iter().map(|c| c.path).collect();
    assert_eq!(paths, ["README.md", "src/lib.rs", "src/new.rs"]);
}

#[test]
fn identical_refs_have_no_changes() {
    let f = diverged();
    let r = f.repo();
    let cmp = r.compare(&spec("feature", "feature")).unwrap();
    assert!(cmp.identical());
    assert!(cmp.ahead.is_empty() && cmp.behind.is_empty());
    assert!(r.changes(cmp.source).unwrap().is_empty());
}

#[test]
fn unrelated_histories_fall_back_to_two_dot() {
    let f = diverged();
    f.git(&["checkout", "-q", "--orphan", "pages"]);
    f.git(&["rm", "-rfq", "."]);
    f.write("index.html", "<h1>hi</h1>\n");
    f.commit("pages");
    let cmp = f.repo().compare(&spec("main", "pages")).unwrap();
    assert!(cmp.unrelated());
    assert_eq!(cmp.mode, RangeMode::Compare);
}

#[test]
fn unknown_ref_is_a_readable_error() {
    let f = diverged();
    let err = f.repo().compare(&spec("main", "nope")).err().unwrap().to_string();
    assert!(err.contains("Unknown ref: nope"), "{err}");
}

#[test]
fn file_diff_lines_and_hunks() {
    let f = diverged();
    let r = f.repo();
    let cmp = r.compare(&spec("main", "feature")).unwrap();
    let changes = r.changes(cmp.source).unwrap();
    let d = r.file_diff(&changes[0], DiffOptions::default()).unwrap();
    assert!(matches!(d.body, DiffBody::Text));
    assert_eq!(d.hunks.len(), 1);
    let rendered: Vec<String> = d
        .lines
        .iter()
        .map(|l| {
            let sign = match l.kind {
                LineKind::Context => ' ',
                LineKind::Added => '+',
                LineKind::Removed => '-',
            };
            format!("{sign}{}", d.line_text(l))
        })
        .collect();
    assert_eq!(rendered, [" fn a() {}", "-fn b() {}", "+fn b2() {}", " fn c() {}"]);
    assert_eq!(d.lines[1].old_no, Some(2));
    assert_eq!(d.lines[2].new_no, Some(2));
    assert_eq!((d.additions(), d.deletions()), (1, 1));
}

#[test]
fn added_file_is_all_additions() {
    let f = diverged();
    let r = f.repo();
    let cmp = r.compare(&spec("main", "feature")).unwrap();
    let changes = r.changes(cmp.source).unwrap();
    let d = r.file_diff(&changes[1], DiffOptions::default()).unwrap();
    assert!(d.lines.iter().all(|l| l.kind == LineKind::Added && l.old_no.is_none()));
}

#[test]
fn rename_is_detected_with_similarity() {
    let f = Fixture::new();
    let body: String = (0..40).map(|i| format!("line {i}\n")).collect();
    f.write("old.rs", &body);
    f.commit("init");
    f.git(&["checkout", "-q", "-b", "mv"]);
    f.git(&["mv", "old.rs", "new.rs"]);
    f.write("new.rs", &body.replace("line 5\n", "line five\n"));
    f.commit("move");
    let r = f.repo();
    let cmp = r.compare(&spec("main", "mv")).unwrap();
    let changes = r.changes(cmp.source).unwrap();
    assert_eq!(changes.len(), 1);
    let c = &changes[0];
    assert_eq!(c.status, ChangeStatus::Renamed);
    assert_eq!(c.old_path.as_deref(), Some("old.rs"));
    assert_eq!(c.path, "new.rs");
    assert!(c.similarity.unwrap() >= 90, "{:?}", c.similarity);
    let d = r.file_diff(c, DiffOptions::default()).unwrap();
    assert_eq!((d.additions(), d.deletions()), (1, 1));
}

#[test]
fn binary_files_are_flagged_not_diffed() {
    let f = Fixture::new();
    fs::write(f.path().join("img.bin"), [0u8, 1, 2, 0, 255, 0]).unwrap();
    f.commit("bin");
    f.git(&["checkout", "-q", "-b", "b"]);
    fs::write(f.path().join("img.bin"), [0u8, 9, 9, 0, 255, 0, 7]).unwrap();
    f.commit("bin2");
    let r = f.repo();
    let changes = r.changes(r.compare(&spec("main", "b")).unwrap().source).unwrap();
    assert!(changes[0].binary);
    assert_eq!(changes[0].additions, None);
    let d = r.file_diff(&changes[0], DiffOptions::default()).unwrap();
    assert!(matches!(d.body, DiffBody::Binary));
    assert_eq!((d.old_size, d.new_size), (6, 7));
}

#[test]
fn large_file_is_gated_until_forced() {
    let f = Fixture::new();
    f.write("big.txt", "a\n");
    f.commit("init");
    f.git(&["checkout", "-q", "-b", "big"]);
    let body: String = (0..100_000).map(|i| format!("row {i}\n")).collect();
    f.write("big.txt", &body);
    f.commit("big");
    let r = f.repo();
    let changes = r.changes(r.compare(&spec("main", "big")).unwrap().source).unwrap();
    let opts = DiffOptions { max_bytes: 64 * 1024, ..Default::default() };
    let gated = r.file_diff(&changes[0], opts).unwrap();
    assert!(matches!(gated.body, DiffBody::TooLarge));
    assert!(gated.lines.is_empty());
    let full = r.file_diff(&changes[0], DiffOptions { force: true, ..opts }).unwrap();
    assert_eq!(full.additions(), 100_000);
    assert_eq!(full.lines.last().map(|l| full.line_text(l)), Some("row 99999"));
}

#[test]
fn ignore_whitespace_hides_reindent() {
    let f = Fixture::new();
    f.write("a.py", "def f():\n    return 1\n");
    f.commit("init");
    f.git(&["checkout", "-q", "-b", "ws"]);
    f.write("a.py", "def f():\n        return 1\n");
    f.commit("indent");
    let r = f.repo();
    let changes = r.changes(r.compare(&spec("main", "ws")).unwrap().source).unwrap();
    let normal = r.file_diff(&changes[0], DiffOptions::default()).unwrap();
    assert_eq!(normal.additions(), 1);
    let ws = r
        .file_diff(&changes[0], DiffOptions { ignore_whitespace: true, ..Default::default() })
        .unwrap();
    assert_eq!(ws.additions(), 0);
}

#[test]
fn commit_changes_use_first_parent_and_root_commit_works() {
    let f = diverged();
    let r = f.repo();
    let cmp = r.compare(&spec("main", "feature")).unwrap();
    let (_, changes) = r.commit_changes(cmp.ahead[0].oid).unwrap();
    assert_eq!(changes.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["src/new.rs"]);
    let root = r.resolve("main~1").unwrap();
    let (src, root_changes) = r.commit_changes(root).unwrap();
    assert!(src.old.is_none());
    assert_eq!(root_changes.len(), 2);
}

#[test]
fn no_newline_markers_are_tracked_not_rendered() {
    let f = Fixture::new();
    f.write("x.txt", "a\nb");
    f.commit("init");
    f.git(&["checkout", "-q", "-b", "nl"]);
    f.write("x.txt", "a\nb\n");
    f.commit("nl");
    let r = f.repo();
    let changes = r.changes(r.compare(&spec("main", "nl")).unwrap().source).unwrap();
    let d = r.file_diff(&changes[0], DiffOptions::default()).unwrap();
    assert!(d.old_no_newline);
    assert!(d.lines.iter().all(|l| !d.line_text(l).contains("No newline")));
}

#[test]
fn recent_commits_span_all_local_branches() {
    let f = diverged();
    let r = f.repo();
    let all: Vec<String> = r.recent_commits(10).unwrap().into_iter().map(|c| c.summary).collect();
    for s in ["A: init", "B: main readme", "C: rename b", "D: add new"] {
        assert!(all.iter().any(|x| x == s), "{s} missing from {all:?}");
    }
    assert_eq!(r.recent_commits(2).unwrap().len(), 2);
}

#[test]
fn two_commits_compare_like_branches() {
    let f = diverged();
    let r = f.repo();
    let a = r.resolve("feature~2").unwrap().to_string(); // A
    let d = r.resolve("feature").unwrap().to_string(); // D
    let cmp = r.compare(&spec(&a, &d)).unwrap();
    assert_eq!(cmp.ahead.len(), 2);
    assert!(cmp.behind.is_empty());
    let paths: Vec<_> = r.changes(cmp.source).unwrap().into_iter().map(|c| c.path).collect();
    assert_eq!(paths, ["src/lib.rs", "src/new.rs"]);
    // short SHAs and revspecs resolve too
    assert_eq!(r.resolve(&a[..7]).unwrap().to_string(), a);
    assert!(r.resolve("feature^").is_ok());
}

#[test]
fn hunk_context_is_extracted() {
    assert_eq!(hunk_context("@@ -1,3 +1,4 @@ fn main() {\n"), "fn main() {");
    assert_eq!(hunk_context("@@ -1 +1 @@\n"), "");
}

#[test]
fn generated_paths() {
    assert!(is_generated_path("Cargo.lock"));
    assert!(is_generated_path("web/pnpm-lock.yaml"));
    assert!(is_generated_path("static/app.min.js"));
    assert!(is_generated_path("pkg/dist/index.js"));
    assert!(!is_generated_path("src/main.rs"));
}
