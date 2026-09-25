//! GPUI front-end. One window, one root view (`Kerf`).

mod app;
mod diff_view;
mod picker;
mod scrollbar;
mod sidebar;
mod state;
mod widgets;

pub use app::Kerf;

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
        ToggleTree,
        CopyItem,
        FocusFilter,
        PageUp,
        PageDown,
        Top,
        Bottom,
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenRepo, None),
        KeyBinding::new("cmd-r", Refresh, None),
        KeyBinding::new("cmd-1", PickBase, None),
        KeyBinding::new("cmd-2", PickCompare, None),
        KeyBinding::new("cmd-shift-s", Swap, None),
        KeyBinding::new("cmd-shift-m", ToggleMode, None),
        KeyBinding::new("cmd-shift-f", FilesTab, None),
        KeyBinding::new("cmd-shift-c", CommitsTab, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-q", Quit, None),
        // Navigation works in both normal and text-input mode.
        KeyBinding::new("up", Up, None),
        KeyBinding::new("down", Down, None),
        KeyBinding::new("enter", Confirm, None),
        KeyBinding::new("escape", Cancel, None),
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
        KeyBinding::new("t", ToggleTree, Some("Kerf")),
        KeyBinding::new("y", CopyItem, Some("Kerf")),
        KeyBinding::new("/", FocusFilter, Some("Kerf")),
        KeyBinding::new("pageup", PageUp, Some("Kerf")),
        KeyBinding::new("pagedown", PageDown, Some("Kerf")),
        KeyBinding::new("space", PageDown, Some("Kerf")),
        KeyBinding::new("shift-space", PageUp, Some("Kerf")),
        KeyBinding::new("g", Top, Some("Kerf")),
        KeyBinding::new("shift-g", Bottom, Some("Kerf")),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
}

pub fn open_window(cx: &mut App, path: Option<PathBuf>) {
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
        |window, cx| cx.new(|cx| Kerf::new(path, window, cx)),
    )
    .expect("open window");
    cx.activate(true);
}
