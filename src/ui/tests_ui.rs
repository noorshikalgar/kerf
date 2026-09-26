//! Headless UI integration tests: real Kerf windows on gpui's test platform, driven with
//! real keystrokes through the app keymap, against real git repositories in temp dirs.

use super::app::{DiffState, Kerf, ListRow, Tab, Which};
use super::Launch;
use crate::diff::{Layout, Row};
use crate::git::{DiffBody, RangeMode};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use tempfile::TempDir;

/// Tests never touch the user's real state file.
fn isolate_state() {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    let dir = DIR.get_or_init(|| TempDir::new().unwrap());
    // SAFETY-free on edition 2021; set once, same value for every test.
    std::env::set_var("KERF_STATE_DIR", dir.path());
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn write(dir: &Path, path: &str, content: &str) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

/// main:    A ── B (README edited)
/// feature: A ── C (lib.rs edited, new.rs added, util.rs deleted) ── D (docs/guide.md added)
fn fixture() -> TempDir {
    let t = TempDir::new().unwrap();
    let d = t.path();
    git(d, &["init", "-q", "-b", "main"]);
    write(d, "README.md", "# demo\n");
    write(d, "src/lib.rs", "fn a() {}\nfn b() {}\nfn c() {}\n");
    write(d, "src/util.rs", "pub fn util() {}\n");
    git(d, &["add", "-A"]);
    git(d, &["commit", "-qm", "A: init"]);
    git(d, &["checkout", "-qb", "feature"]);
    write(d, "src/lib.rs", "fn a() {}\nfn b2() {}\nfn c() {}\n");
    write(d, "src/new.rs", "pub fn new() {}\n");
    std::fs::remove_file(d.join("src/util.rs")).unwrap();
    git(d, &["add", "-A"]);
    git(d, &["commit", "-qm", "C: rework lib"]);
    write(d, "docs/guide.md", "hello\n");
    git(d, &["add", "-A"]);
    git(d, &["commit", "-qm", "D: add guide"]);
    git(d, &["checkout", "-q", "main"]);
    write(d, "README.md", "# demo on main\n");
    git(d, &["commit", "-qam", "B: readme"]);
    git(d, &["checkout", "-q", "feature"]);
    t
}

fn boot(cx: &mut TestAppContext, launch: Launch) -> (Entity<Kerf>, &mut VisualTestContext) {
    isolate_state();
    cx.update(super::init);
    let (view, cx) = cx.add_window_view(|window, cx| Kerf::new(launch, window, cx));
    cx.run_until_parked();
    (view, cx)
}

fn open<'a>(cx: &'a mut TestAppContext, path: &Path) -> (Entity<Kerf>, &'a mut VisualTestContext) {
    boot(cx, Launch::Repo(path.to_path_buf()))
}

fn file_paths(k: &Kerf) -> Vec<String> {
    let Some(d) = k.range_data() else { return vec![] };
    k.rows
        .iter()
        .filter_map(|r| match r {
            ListRow::File { change, .. } => Some(d.changes[*change].path.clone()),
            _ => None,
        })
        .collect()
}

fn active_path(k: &Kerf) -> Option<String> {
    k.current_target().map(|t| t.change.path.clone())
}

fn ready(k: &Kerf) -> bool {
    matches!(k.diff, DiffState::Ready(_))
}

// ───────────────────────────── repository & range ─────────────────────────────

#[gpui::test]
fn opening_a_repo_picks_default_range_and_shows_first_file(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    view.update(cx, |k, _| {
        assert!(k.repo_path.is_some());
        assert_eq!(k.base.as_deref(), Some("main"));
        assert_eq!(k.compare.as_deref(), Some("feature"));
        // PR Merge View: README (changed only on main) is not part of the range.
        assert_eq!(file_paths(k), ["docs/guide.md", "src/lib.rs", "src/new.rs", "src/util.rs"]);
        let d = k.range_data().unwrap();
        assert_eq!((d.cmp.ahead.len(), d.cmp.behind.len()), (2, 1));
        assert!(ready(k), "first file's diff is loaded");
        assert_eq!(active_path(k).as_deref(), Some("docs/guide.md"));
    });
}

#[gpui::test]
fn not_a_repository_shows_an_error_and_the_start_page(cx: &mut TestAppContext) {
    let dir = TempDir::new().unwrap();
    let (view, cx) = open(cx, dir.path());
    view.update(cx, |k, _| {
        assert!(k.repo_path.is_none());
        assert!(k.repo_error.as_deref().unwrap_or("").contains("Not a Git repository"));
    });
}

#[gpui::test]
fn compare_view_includes_base_side_changes(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("secondary-shift-m");
    view.update(cx, |k, _| {
        assert_eq!(k.mode, RangeMode::Compare);
        assert!(file_paths(k).contains(&"README.md".to_string()));
    });
    cx.simulate_keystrokes("secondary-shift-m");
    view.update(cx, |k, _| assert_eq!(k.mode, RangeMode::PrMerge));
}

#[gpui::test]
fn swap_flips_base_and_compare(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("secondary-shift-s");
    view.update(cx, |k, _| {
        assert_eq!((k.base.as_deref(), k.compare.as_deref()), (Some("feature"), Some("main")));
        let d = k.range_data().unwrap();
        assert_eq!((d.cmp.ahead.len(), d.cmp.behind.len()), (1, 2));
    });
}

#[gpui::test]
fn refresh_picks_up_new_commits(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    write(repo.path(), "src/extra.rs", "x\n");
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-qm", "E: extra"]);
    cx.simulate_keystrokes("secondary-r");
    view.update(cx, |k, _| {
        assert!(file_paths(k).contains(&"src/extra.rs".to_string()));
        assert_eq!(k.range_data().unwrap().cmp.ahead.len(), 3);
    });
}

// ───────────────────────────── navigation & diff views ─────────────────────────────

#[gpui::test]
fn arrow_keys_walk_files_and_the_diff_follows(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("t"); // flat list: every row is a file
    cx.simulate_keystrokes("down");
    view.update(cx, |k, _| assert_eq!(active_path(k).as_deref(), Some("src/lib.rs")));
    cx.simulate_keystrokes("j j");
    view.update(cx, |k, _| assert_eq!(active_path(k).as_deref(), Some("src/util.rs")));
    cx.simulate_keystrokes("k");
    view.update(cx, |k, _| {
        assert_eq!(active_path(k).as_deref(), Some("src/new.rs"));
        let DiffState::Ready(l) = &k.diff else { panic!("diff not ready") };
        assert!(l.fd.lines.iter().all(|l| l.kind == crate::git::LineKind::Added));
    });
}

#[gpui::test]
fn split_and_whitespace_toggles(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("t down"); // src/lib.rs: one changed line
    cx.simulate_keystrokes("s");
    view.update(cx, |k, _| {
        assert_eq!(k.layout, Layout::Split);
        let DiffState::Ready(l) = &k.diff else { panic!() };
        assert!(l.rows.rows.iter().any(|r| matches!(r, Row::Pair { .. })));
    });
    cx.simulate_keystrokes("s w");
    view.update(cx, |k, _| {
        assert_eq!(k.layout, Layout::Unified);
        assert!(k.ignore_ws);
        assert!(ready(k));
    });
}

#[gpui::test]
fn preview_tab_is_replaced_and_enter_pins_it(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("t down down");
    view.update(cx, |k, _| assert_eq!(k.tabs.len(), 1, "preview tab reused"));
    cx.simulate_keystrokes("enter down");
    view.update(cx, |k, _| {
        assert_eq!(k.tabs.len(), 2, "pinned tab kept, new preview opened");
        assert!(k.tabs[0].pinned);
        assert_eq!(active_path(k).as_deref(), Some("src/util.rs"));
    });
    cx.simulate_keystrokes("secondary-shift-[");
    view.update(cx, |k, _| assert_eq!(active_path(k).as_deref(), Some("src/new.rs")));
    cx.simulate_keystrokes("secondary-w");
    view.update(cx, |k, _| {
        assert_eq!(k.tabs.len(), 1);
        assert_eq!(active_path(k).as_deref(), Some("src/util.rs"));
    });
}

#[gpui::test]
fn filter_narrows_the_file_list(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("/");
    cx.simulate_input("new");
    view.update(cx, |k, _| assert_eq!(file_paths(k), ["src/new.rs"]));
    cx.simulate_keystrokes("escape escape");
    view.update(cx, |k, _| assert_eq!(file_paths(k).len(), 4));
}

// ───────────────────────────── commits ─────────────────────────────

#[gpui::test]
fn commits_tab_opens_a_commits_own_files(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("secondary-shift-c");
    view.update(cx, |k, cx| {
        assert_eq!(k.tab, Tab::Commits);
        let ix = k.rows.iter().position(|r| matches!(r, ListRow::Commit { .. })).unwrap();
        k.select_row(ix, cx); // newest ahead commit: D
    });
    cx.run_until_parked();
    view.update(cx, |k, _| {
        let t = k.current_target().expect("commit file shown");
        assert_eq!(t.commit.as_ref().map(|c| c.summary.as_str()), Some("D: add guide"));
        assert_eq!(t.change.path, "docs/guide.md");
    });
}

#[gpui::test]
fn a_commit_can_become_the_base(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("secondary-shift-c");
    // Select C (second ahead commit) and make it the base: range is now just D.
    view.update(cx, |k, _| {
        let ix = k.rows.iter().enumerate().filter(|(_, r)| matches!(r, ListRow::Commit { .. })).nth(1).unwrap().0;
        k.selected = Some(ix);
    });
    cx.simulate_keystrokes("b");
    view.update(cx, |k, _| {
        let base = k.base.clone().unwrap();
        assert_eq!(base.len(), 40, "base is a commit sha");
        assert_eq!(k.describe(&base).subject.as_deref(), Some("C: rework lib"));
        let d = k.range_data().unwrap();
        assert_eq!(d.cmp.ahead.len(), 1);
        assert_eq!(d.changes.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["docs/guide.md"]);
    });
}

#[gpui::test]
fn picker_sets_compare_from_a_typed_query(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("secondary-2");
    cx.simulate_input("mai");
    cx.simulate_keystrokes("enter");
    view.update(cx, |k, _| {
        assert!(k.picker.is_none());
        assert_eq!(k.compare.as_deref(), Some("main"));
        let d = k.range_data().unwrap();
        assert!(d.cmp.identical(), "main vs main");
    });
}

// ───────────────────────────── large files ─────────────────────────────

#[gpui::test]
fn large_file_is_gated_until_loaded(cx: &mut TestAppContext) {
    let repo = fixture();
    let big: String = (0..1_200_000).map(|i| format!("{i}\n")).collect(); // > 8 MB
    let big = big.repeat(3); // > 20 MB
    write(repo.path(), "big.txt", &big);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-qm", "big"]);
    let (view, cx) = open(cx, repo.path());
    view.update(cx, |k, cx| {
        let ix = k.rows.iter().position(|r| matches!(r, ListRow::File { change, .. } if k.range_data().unwrap().changes[*change].path == "big.txt")).unwrap();
        k.select_row(ix, cx);
    });
    cx.run_until_parked();
    view.update(cx, |k, _| {
        let DiffState::Ready(l) = &k.diff else { panic!() };
        assert!(matches!(l.fd.body, DiffBody::TooLarge));
    });
    cx.simulate_keystrokes("enter");
    view.update(cx, |k, _| {
        let DiffState::Ready(l) = &k.diff else { panic!() };
        assert!(matches!(l.fd.body, DiffBody::Text));
        assert!(l.fd.lines.len() > 3_000_000);
    });
}

// ───────────────────────────── plain diffs ─────────────────────────────

fn scratch_editors(
    view: &Entity<Kerf>,
    cx: &mut VisualTestContext,
) -> (Entity<super::editor::Editor>, Entity<super::editor::Editor>) {
    view.update_in(cx, |k, window, cx| {
        let sc = k.active_scratch().expect("scratch tab active");
        k.ensure_editors(&sc, window, cx);
        let p = k.editors.get(&sc.id).unwrap();
        (p.left.clone(), p.right.clone())
    })
}

#[gpui::test]
fn new_diff_is_live_and_enter_types_a_newline(cx: &mut TestAppContext) {
    let (view, cx) = boot(cx, Launch::Empty);
    cx.simulate_keystrokes("secondary-n");
    let (left, right) = scratch_editors(&view, cx);
    // Left editor has focus: type two lines, Enter must insert a newline (regression).
    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("world");
    left.update(cx, |e, _| assert_eq!(e.buffer.text(), "hello\nworld"));
    // Right side: same first line, different second → one change pair.
    right.update_in(cx, |e, window, _| e.focus(window));
    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("there");
    view.update(cx, |k, _| {
        let sc = k.active_scratch().unwrap();
        let a = k.editors[&sc.id].aligned.clone().unwrap();
        assert_eq!((a.additions, a.deletions), (1, 1));
        assert_eq!(a.rows(), 2);
    });
    // Undo on the right restores "hello" only; the diff updates live.
    cx.simulate_keystrokes("secondary-z");
    right.update(cx, |e, _| assert_ne!(e.buffer.text(), "hello\nthere"));
}

#[gpui::test]
fn empty_side_starts_at_the_top(cx: &mut TestAppContext) {
    let (view, cx) = boot(cx, Launch::Empty);
    cx.simulate_keystrokes("secondary-n");
    let (_left, _right) = scratch_editors(&view, cx);
    cx.simulate_input("a");
    cx.simulate_keystrokes("enter enter enter");
    view.update(cx, |k, _| {
        let sc = k.active_scratch().unwrap();
        let a = k.editors[&sc.id].aligned.clone().unwrap();
        assert_eq!(a.right[0].line, Some(0), "right's only line is on the first row");
    });
}

#[gpui::test]
fn two_files_open_as_an_editable_live_diff(cx: &mut TestAppContext) {
    let dir = TempDir::new().unwrap();
    let (a, b) = (dir.path().join("a.txt"), dir.path().join("b.txt"));
    std::fs::write(&a, "one\ntwo\nthree\n").unwrap();
    std::fs::write(&b, "one\n2\nthree\n").unwrap();
    let (view, cx) = boot(cx, Launch::Files(a.clone(), b.clone()));
    let (left, _right) = scratch_editors(&view, cx);
    view.update(cx, |k, _| {
        let sc = k.active_scratch().unwrap();
        let al = k.editors[&sc.id].aligned.clone().unwrap();
        assert_eq!((al.additions, al.deletions), (1, 1));
        assert_eq!(k.tabs[0].target.change.path, "a.txt ↔ b.txt");
    });
    // Editing is in memory only: the file on disk is untouched.
    left.update_in(cx, |e, window, _| e.focus(window));
    cx.simulate_keystrokes("secondary-shift-backspace");
    left.update(cx, |e, _| assert!(e.buffer.is_empty()));
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "one\ntwo\nthree\n");
}

#[gpui::test]
fn identical_plain_texts_are_reported_identical(cx: &mut TestAppContext) {
    let (view, cx) = boot(cx, Launch::Empty);
    cx.simulate_keystrokes("secondary-n");
    let (left, right) = scratch_editors(&view, cx);
    left.update(cx, |e, cx| e.set_text("same\ntext", cx));
    right.update(cx, |e, cx| e.set_text("same\ntext", cx));
    cx.run_until_parked();
    view.update(cx, |k, _| {
        let sc = k.active_scratch().unwrap();
        assert!(k.editors[&sc.id].aligned.clone().unwrap().identical());
    });
}

// ───────────────────────────── windows & modes ─────────────────────────────

#[gpui::test]
fn closing_the_repo_keeps_plain_diff_tabs(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("enter"); // pin the git tab
    cx.simulate_keystrokes("secondary-n");
    view.update(cx, |k, cx| {
        assert_eq!(k.tabs.len(), 2);
        k.close_repo(cx);
    });
    cx.run_until_parked();
    view.update(cx, |k, _| {
        assert!(k.repo_path.is_none());
        assert_eq!(k.tabs.len(), 1);
        assert!(k.tabs[0].target.scratch.is_some());
        assert!(k.base.is_none() && k.compare.is_none());
    });
}

#[gpui::test]
fn start_page_when_launched_empty(cx: &mut TestAppContext) {
    let (view, cx) = boot(cx, Launch::Empty);
    view.update(cx, |k, _| {
        assert!(k.repo_path.is_none());
        assert!(k.tabs.is_empty());
        assert!(matches!(k.diff, DiffState::Empty));
    });
}

#[gpui::test]
fn picker_opens_for_base_and_closes_with_escape(cx: &mut TestAppContext) {
    let repo = fixture();
    let (view, cx) = open(cx, repo.path());
    cx.simulate_keystrokes("secondary-1");
    view.update(cx, |k, _| assert_eq!(k.picker.as_ref().map(|p| p.which), Some(Which::Base)));
    cx.simulate_keystrokes("escape");
    view.update(cx, |k, _| assert!(k.picker.is_none()));
}

#[gpui::test]
fn repo_switcher_filters_and_opens_by_keyboard(cx: &mut TestAppContext) {
    let (a, b) = (fixture(), fixture());
    let (view, cx) = open(cx, a.path());
    let b_path = b.path().canonicalize().unwrap();
    view.update(cx, |k, cx| {
        k.persisted.recents = vec![k.repo_path.clone().unwrap(), b_path.clone()];
        k.open_repo_menu(cx);
        // Preselects the first repo that isn't the current one.
        assert_eq!(k.repo_menu_items()[k.repo_sel], super::welcome::RepoItem::Recent(b_path.clone()));
    });
    let b_name = b_path.file_name().unwrap().to_string_lossy().to_string();
    cx.simulate_input(&b_name);
    view.update(cx, |k, _| {
        let recents =
            k.repo_menu_items().into_iter().filter(|i| matches!(i, super::welcome::RepoItem::Recent(_))).count();
        assert_eq!(recents, 1, "typing filters the list");
    });
    cx.simulate_keystrokes("enter");
    view.update(cx, |k, _| {
        assert!(!k.repo_menu_open);
        assert_eq!(k.repo_path.as_deref(), Some(b_path.as_path()));
    });
    // Escape closes without changing anything.
    view.update(cx, |k, cx| k.open_repo_menu(cx));
    cx.simulate_keystrokes("escape");
    view.update(cx, |k, _| assert!(!k.repo_menu_open));
}

/// Wheel over a modal must not scroll the diff behind it.
#[gpui::test]
fn popups_block_scrolling_the_view_behind(cx: &mut TestAppContext) {
    let repo = fixture();
    let long: String = (0..600).map(|i| format!("line {i}\n")).collect();
    write(repo.path(), "long.txt", &long);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-qm", "long"]);
    let (view, cx) = open(cx, repo.path());
    view.update(cx, |k, cx| {
        let ix = k
            .rows
            .iter()
            .position(|r| matches!(r, ListRow::File { change, .. } if k.range_data().unwrap().changes[*change].path == "long.txt"))
            .unwrap();
        k.select_row(ix, cx);
    });
    cx.run_until_parked();
    let offset = |view: &Entity<Kerf>, cx: &mut VisualTestContext| {
        view.update(cx, |k, _| f32::from(k.diff_scroll.0.borrow().base_handle.offset().y))
    };
    let wheel = |cx: &mut VisualTestContext| {
        let area = cx.update(|window, _| window.bounds().center());
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: area,
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-400.))),
            ..Default::default()
        });
        cx.run_until_parked();
    };
    // Control: without a popup, the wheel scrolls the diff.
    let before = offset(&view, cx);
    wheel(cx);
    let scrolled = offset(&view, cx);
    assert!(scrolled < before, "diff scrolls normally ({before} -> {scrolled})");
    // With the shortcuts popup open, the same wheel must not move it.
    view.update(cx, |k, cx| {
        k.shortcuts_open = true;
        cx.notify();
    });
    cx.run_until_parked();
    wheel(cx);
    assert_eq!(offset(&view, cx), scrolled, "diff behind the popup must not scroll");
}
