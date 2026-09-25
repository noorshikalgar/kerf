//! GPUI front-end. One window, one root view (`Kerf`).

mod app;
mod diff_view;
mod editor;
mod info;
mod picker;
mod scratch;
mod shortcuts;
mod scrollbar;
mod sidebar;
mod state;
mod welcome;
mod widgets;

pub use app::Kerf;

/// What the app opens on start.
pub enum Launch {
    /// Last repository, if any.
    Default,
    Repo(PathBuf),
    /// `kerf a.txt b.txt` — plain diff of two files.
    Files(PathBuf, PathBuf),
}

use gpui::{
    actions, point, px, size, App, AppContext as _, Bounds, KeyBinding, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use std::path::PathBuf;

actions!(
    kerf,
    [
        OpenRepo,
        Refresh,
        PickBase,
        PickCompare,
        Swap,
        ToggleMode,
        FilesTab,
        CommitsTab,
        ToggleSidebar,
        Quit,
        Up,
        Down,
        Left,
        Right,
        Confirm,
        Cancel,
        NextFile,
        PrevFile,
        NextHunk,
        PrevHunk,
        ToggleSplit,
        ToggleWhitespace,
        ToggleWrap,
        ToggleTree,
        CopyItem,
        SetBase,
        SetCompare,
        CloseTab,
        NextTab,
        PrevTab,
        ShowInfo,
        NewDiff,
        CompareFiles,
        Paste,
        ShowShortcuts,
        FocusFilter,
        PageUp,
        PageDown,
        Top,
        Bottom,
    ]
);

/// App-wide bindings are scoped to the root view's contexts. A binding with *no* context
/// matches at the deepest level in gpui and would override the editor's Enter / arrows / ⌘V;
/// scoped, the editor's (deeper) `KerfEditor` bindings win while it has focus.
const ROOT: &str = "Kerf || KerfInput";

/// All app-level key bindings.
fn app_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-o", OpenRepo, Some(ROOT)),
        KeyBinding::new("cmd-r", Refresh, Some(ROOT)),
        KeyBinding::new("cmd-1", PickBase, Some(ROOT)),
        KeyBinding::new("cmd-2", PickCompare, Some(ROOT)),
        KeyBinding::new("cmd-shift-s", Swap, Some(ROOT)),
        KeyBinding::new("cmd-shift-m", ToggleMode, Some(ROOT)),
        KeyBinding::new("cmd-shift-f", FilesTab, Some(ROOT)),
        KeyBinding::new("cmd-shift-c", CommitsTab, Some(ROOT)),
        KeyBinding::new("cmd-b", ToggleSidebar, Some(ROOT)),
        KeyBinding::new("cmd-q", Quit, Some(ROOT)),
        KeyBinding::new("cmd-w", CloseTab, Some(ROOT)),
        KeyBinding::new("cmd-shift-]", NextTab, Some(ROOT)),
        KeyBinding::new("cmd-shift-[", PrevTab, Some(ROOT)),
        KeyBinding::new("ctrl-tab", NextTab, Some(ROOT)),
        KeyBinding::new("ctrl-shift-tab", PrevTab, Some(ROOT)),
        KeyBinding::new("f1", ShowInfo, Some(ROOT)),
        KeyBinding::new("cmd-n", NewDiff, Some(ROOT)),
        KeyBinding::new("cmd-shift-n", CompareFiles, Some(ROOT)),
        KeyBinding::new("cmd-v", Paste, Some(ROOT)),
        KeyBinding::new("cmd-/", ShowShortcuts, Some(ROOT)),
        // Navigation works in both normal and text-input mode.
        KeyBinding::new("up", Up, Some(ROOT)),
        KeyBinding::new("down", Down, Some(ROOT)),
        KeyBinding::new("enter", Confirm, Some(ROOT)),
        KeyBinding::new("escape", Cancel, Some(ROOT)),
        // Single-letter keys only when no text input is active.
        KeyBinding::new("k", Up, Some("Kerf")),
        KeyBinding::new("j", Down, Some("Kerf")),
        KeyBinding::new("left", Left, Some("Kerf")),
        KeyBinding::new("right", Right, Some("Kerf")),
        KeyBinding::new("h", Left, Some("Kerf")),
        KeyBinding::new("l", Right, Some("Kerf")),
        KeyBinding::new("]", NextFile, Some("Kerf")),
        KeyBinding::new("[", PrevFile, Some("Kerf")),
        KeyBinding::new("n", NextHunk, Some("Kerf")),
        KeyBinding::new("p", PrevHunk, Some("Kerf")),
        KeyBinding::new("s", ToggleSplit, Some("Kerf")),
        KeyBinding::new("w", ToggleWhitespace, Some("Kerf")),
        KeyBinding::new("z", ToggleWrap, Some("Kerf")),
        KeyBinding::new("t", ToggleTree, Some("Kerf")),
        KeyBinding::new("y", CopyItem, Some("Kerf")),
        KeyBinding::new("b", SetBase, Some("Kerf")),
        KeyBinding::new("c", SetCompare, Some("Kerf")),
        KeyBinding::new("?", ShowInfo, Some("Kerf")),
        KeyBinding::new("shift-/", ShowInfo, Some("Kerf")),
        KeyBinding::new("/", FocusFilter, Some("Kerf")),
        KeyBinding::new("pageup", PageUp, Some("Kerf")),
        KeyBinding::new("pagedown", PageDown, Some("Kerf")),
        KeyBinding::new("space", PageDown, Some("Kerf")),
        KeyBinding::new("shift-space", PageUp, Some("Kerf")),
        KeyBinding::new("g", Top, Some("Kerf")),
        KeyBinding::new("shift-g", Bottom, Some("Kerf")),
    ]
}

pub fn init(cx: &mut App) {
    editor::init(cx);
    cx.bind_keys(app_bindings());
    cx.on_action(|_: &Quit, cx| cx.quit());
}

pub fn open_window(cx: &mut App, launch: Launch) {
    let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Kerf".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.), px(10.))),
            }),
            window_background: WindowBackgroundAppearance::Opaque,
            window_min_size: Some(size(px(900.), px(600.))),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| Kerf::new(launch, window, cx)),
    )
    .expect("open window");
    cx.activate(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{KeyContext, Keymap, Keystroke};

    fn resolve(key: &str, contexts: &[&str]) -> String {
        let mut bindings = editor::bindings();
        bindings.extend(app_bindings());
        let keymap = Keymap::new(bindings);
        let stack: Vec<KeyContext> = contexts.iter().map(|c| KeyContext::parse(c).unwrap()).collect();
        let (found, _) = keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], &stack);
        found.first().map(|b| b.action().name().to_string()).unwrap_or_default()
    }

    #[test]
    fn editor_keys_beat_app_keys_while_editing() {
        let editing = ["KerfInput", "KerfEditor"];
        assert_eq!(resolve("enter", &editing), "kerf_editor::Newline");
        assert_eq!(resolve("up", &editing), "kerf_editor::Up");
        assert_eq!(resolve("cmd-v", &editing), "kerf_editor::Paste");
        assert_eq!(resolve("escape", &editing), "kerf_editor::Blur");
        // App shortcuts without an editor binding still work while typing.
        assert_eq!(resolve("cmd-w", &editing), "kerf::CloseTab");
    }

    #[test]
    fn app_keys_work_outside_the_editor() {
        assert_eq!(resolve("enter", &["Kerf"]), "kerf::Confirm");
        assert_eq!(resolve("j", &["Kerf"]), "kerf::Down");
        // Single-letter app keys are off while a text input is active.
        assert_eq!(resolve("j", &["KerfInput"]), "");
    }
}
