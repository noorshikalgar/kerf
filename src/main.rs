use gpui::Application;
use kerf::ui::Launch;
use std::path::PathBuf;

fn main() {
    let args: Vec<PathBuf> = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect();
    // `kerf <repo>` opens a repository; `kerf <file> <file>` opens a plain diff.
    let launch = match args.as_slice() {
        [a, b] if a.is_file() && b.is_file() => Launch::Files(a.clone(), b.clone()),
        [p, ..] => Launch::Repo(p.clone()),
        [] => Launch::Default,
    };
    Application::new().run(move |cx| {
        kerf::ui::init(cx);
        kerf::ui::open_window(cx, launch);
    });
}
