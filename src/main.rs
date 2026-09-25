use gpui::Application;
use std::path::PathBuf;

fn main() {
    let path = std::env::args().nth(1).map(PathBuf::from).map(|p| p.canonicalize().unwrap_or(p));
    Application::new().run(move |cx| {
        kerf::ui::init(cx);
        kerf::ui::open_window(cx, path);
    });
}
