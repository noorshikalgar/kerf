//! GPUI front-end. One window, one root view (`Kerf`).

mod app;
mod diff_view;
mod editor;
mod info;
mod picker;
mod scratch;
mod scrollbar;
mod shortcuts;
mod sidebar;
mod state;
mod welcome;
mod widgets;

pub use app::Kerf;

/// What the app opens on start.
pub enum Launch {
    /// Last repository, if any.
    Default,
    /// Start page (new windows).
    Empty,
    Repo(PathBuf),
    /// `kerf a.txt b.txt` — plain diff of two files.
    Files(PathBuf, PathBuf),
}

use gpui::{
    actions, point, px, size, App, AppContext as _, Bounds, KeyBinding, Menu, MenuItem, SystemMenuType,
    TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions,
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
        NewWindow,
        CloseWindow,
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
/// Single-key app shortcuts (j k n p s w …): only while no text input is active *and* no
/// editor is in the focus path (`!` checks the whole context stack, so this holds even on
/// the first frame after focus moves into an editor).
const LETTERS: &str = "Kerf && !KerfEditor";

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
        KeyBinding::new("alt-cmd-n", CompareFiles, Some(ROOT)),
        KeyBinding::new("cmd-shift-n", NewWindow, Some(ROOT)),
        KeyBinding::new("cmd-shift-w", CloseWindow, Some(ROOT)),
        KeyBinding::new("cmd-v", Paste, Some(ROOT)),
        KeyBinding::new("cmd-/", ShowShortcuts, Some(ROOT)),
        // Navigation works in both normal and text-input mode.
        KeyBinding::new("up", Up, Some(ROOT)),
        KeyBinding::new("down", Down, Some(ROOT)),
        KeyBinding::new("enter", Confirm, Some(ROOT)),
        KeyBinding::new("escape", Cancel, Some(ROOT)),
        // Single-letter keys only when no text input is active.
        KeyBinding::new("k", Up, Some(LETTERS)),
        KeyBinding::new("j", Down, Some(LETTERS)),
        KeyBinding::new("left", Left, Some(LETTERS)),
        KeyBinding::new("right", Right, Some(LETTERS)),
        KeyBinding::new("h", Left, Some(LETTERS)),
        KeyBinding::new("l", Right, Some(LETTERS)),
        KeyBinding::new("]", NextFile, Some(LETTERS)),
        KeyBinding::new("[", PrevFile, Some(LETTERS)),
        KeyBinding::new("n", NextHunk, Some(LETTERS)),
        KeyBinding::new("p", PrevHunk, Some(LETTERS)),
        KeyBinding::new("s", ToggleSplit, Some(LETTERS)),
        KeyBinding::new("w", ToggleWhitespace, Some(LETTERS)),
        KeyBinding::new("z", ToggleWrap, Some(LETTERS)),
        KeyBinding::new("t", ToggleTree, Some(LETTERS)),
        KeyBinding::new("y", CopyItem, Some(LETTERS)),
        KeyBinding::new("b", SetBase, Some(LETTERS)),
        KeyBinding::new("c", SetCompare, Some(LETTERS)),
        KeyBinding::new("?", ShowInfo, Some(LETTERS)),
        KeyBinding::new("shift-/", ShowInfo, Some(LETTERS)),
        KeyBinding::new("/", FocusFilter, Some(LETTERS)),
        KeyBinding::new("pageup", PageUp, Some(LETTERS)),
        KeyBinding::new("pagedown", PageDown, Some(LETTERS)),
        KeyBinding::new("space", PageDown, Some(LETTERS)),
        KeyBinding::new("shift-space", PageUp, Some(LETTERS)),
        KeyBinding::new("g", Top, Some(LETTERS)),
        KeyBinding::new("shift-g", Bottom, Some(LETTERS)),
    ]
}

pub fn init(cx: &mut App) {
    editor::init(cx);
    cx.bind_keys(app_bindings());
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &NewWindow, cx| open_window(cx, Launch::Empty));
    // Quit when the last window closes.
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    set_menus(cx);
}

fn set_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            name: "Kerf".into(),
            items: vec![
                MenuItem::action("Keyboard Shortcuts", ShowShortcuts),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Kerf", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Window", NewWindow),
                MenuItem::action("New Diff", NewDiff),
                MenuItem::action("Compare Files…", CompareFiles),
                MenuItem::separator(),
                MenuItem::action("Open Repository…", OpenRepo),
                MenuItem::action("Refresh", Refresh),
                MenuItem::separator(),
                MenuItem::action("Close Tab", CloseTab),
                MenuItem::action("Close Window", CloseWindow),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::action("Split / Unified", ToggleSplit),
                MenuItem::action("Wrap Lines", ToggleWrap),
                MenuItem::action("Ignore Whitespace", ToggleWhitespace),
                MenuItem::separator(),
                MenuItem::action("PR Merge / Compare View", ToggleMode),
                MenuItem::action("How the Views Differ", ShowInfo),
            ],
        },
    ]);
}

pub fn open_window(cx: &mut App, launch: Launch) {
    // Cascade new windows so they don't stack exactly on top of each other.
    let n = cx.windows().len() as f32;
    let mut bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
    bounds.origin.x += px(28. * n);
    bounds.origin.y += px(28. * n);
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
        assert_eq!(resolve("cmd-shift-backspace", &editing), "kerf_editor::ClearAll");
        assert_eq!(resolve("alt-down", &editing), "kerf_editor::MoveLineDown");
        // App shortcuts without an editor binding still work while typing.
        assert_eq!(resolve("cmd-w", &editing), "kerf::CloseTab");
        assert_eq!(resolve("cmd-shift-n", &editing), "kerf::NewWindow");
        assert_eq!(resolve("alt-cmd-n", &["Kerf"]), "kerf::CompareFiles");
    }

    #[test]
    fn app_keys_work_outside_the_editor() {
        assert_eq!(resolve("enter", &["Kerf"]), "kerf::Confirm");
        assert_eq!(resolve("j", &["Kerf"]), "kerf::Down");
        // Single-letter app keys are off while a text input is active.
        assert_eq!(resolve("j", &["KerfInput"]), "");
        // Even if the root still says "Kerf" (focus just moved), an editor in the stack wins.
        assert_eq!(resolve("h", &["Kerf", "KerfEditor"]), "");
    }
}

#[cfg(test)]
mod tests_ui;
