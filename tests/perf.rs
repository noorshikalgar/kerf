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
        .map(|i| {
            if i % 3 == 0 {
                format!("fn f{i}(x: u64) -> u64 {{ x * {i} }} // line {i}\n")
            } else {
                format!("fn f{i}(x: u32) -> u32 {{ x + {i} }} // line {i}\n")
            }
        })
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

#[test]
#[ignore]
fn live_diff_alignment_and_wrap_at_scale() {
    // 50k-line texts with a change every 7th line: the live editor's background path.
    let old: Vec<String> = (0..50_000).map(|i| format!("line {i} of the document")).collect();
    let new: Vec<String> =
        old.iter().enumerate().map(|(i, l)| if i % 7 == 0 { format!("{l} (edited)") } else { l.clone() }).collect();
    let t = Instant::now();
    let a = kerf::align::align_within(&old, &new, false, std::time::Duration::from_secs(5));
    println!("align 50k: {:?} ({} rows, +{} −{})", t.elapsed(), a.rows(), a.additions, a.deletions);
    assert_eq!(a.left.len(), a.right.len());
    // Exact, not a deadline fallback: one edited line in every 7.
    assert_eq!((a.additions, a.deletions), (7143, 7143));

    // Soft-wrap layout for a 100k-row diff at 80 columns.
    let text: String = (0..100_000).map(|i| format!("{} {}\n", "word ".repeat(i % 40), i)).collect();
    let fd = kerf::git::diff_buffers(b"", text.as_bytes(), "a", "b", DiffOptions { force: true, ..Default::default() })
        .unwrap();
    let rows = diff::build(&fd, Layout::Unified);
    let t = Instant::now();
    let w = diff::wrap_rows(&fd, &rows, 80);
    println!("wrap 100k rows: {:?} ({} visual rows)", t.elapsed(), w.map.len());
}
