//! Large-diff timing check. Run: `cargo test --release --test perf -- --ignored --nocapture`

use kerf::diff::{self, Layout};
use kerf::git::{DiffOptions, RangeMode, RangeSpec, Repo};
use std::process::Command;
use std::time::Instant;

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
#[ignore]
fn hundred_k_line_rust_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let p = dir.path();
    git(p, &["init", "-q", "-b", "main"]);
    let old: String = (0..100_000).map(|i| format!("fn f{i}(x: u32) -> u32 {{ x + {i} }} // line {i}\n")).collect();
    std::fs::write(p.join("big.rs"), &old).unwrap();
    git(p, &["add", "-A"]);
    git(p, &["commit", "-qm", "a"]);
    git(p, &["checkout", "-qb", "b"]);
    let new: String = (0..100_000)
        .map(|i| if i % 3 == 0 { format!("fn f{i}(x: u64) -> u64 {{ x * {i} }} // line {i}\n") } else { format!("fn f{i}(x: u32) -> u32 {{ x + {i} }} // line {i}\n") })
        .collect();
    std::fs::write(p.join("big.rs"), &new).unwrap();
    git(p, &["commit", "-qam", "b"]);

    let t = Instant::now();
    let repo = Repo::open(p).unwrap();
    let cmp = repo.compare(&RangeSpec { base: "main".into(), compare: "b".into(), mode: RangeMode::PrMerge }).unwrap();
    let changes = repo.changes(cmp.source).unwrap();
    println!("changes: {:?}", t.elapsed());

    let t = Instant::now();
    let fd = repo.file_diff(&changes[0], DiffOptions::default()).unwrap();
    println!("file_diff: {:?} ({} lines, {} hunks)", t.elapsed(), fd.lines.len(), fd.hunks.len());

    let t = Instant::now();
    let rows = diff::build(&fd, Layout::Unified);
    println!("rows unified: {:?} ({} rows)", t.elapsed(), rows.rows.len());
    let t = Instant::now();
    let _ = diff::build(&fd, Layout::Split);
    println!("rows split: {:?}", t.elapsed());

    let t = Instant::now();
    let hl = kerf::highlight::highlight(&fd);
    println!("highlight: {:?} (some={})", t.elapsed(), hl.is_some());
}
