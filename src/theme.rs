//! Black Metal theme. Every colour and size used by the UI lives here (docs/03-style.md).

use gpui::{px, rgb, Hsla, Pixels};

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

// Surfaces
pub fn void() -> Hsla {
    c(0x000000)
}
pub fn abyss() -> Hsla {
    c(0x0a0a0a)
}
pub fn crypt() -> Hsla {
    c(0x111111)
}
pub fn ash() -> Hsla {
    c(0x1a1a1a)
}
pub fn slate() -> Hsla {
    c(0x222222)
}
pub fn line() -> Hsla {
    c(0x1c1c1c)
}
pub fn line_hi() -> Hsla {
    c(0x2e2e2e)
}

// Ink
pub fn bone() -> Hsla {
    c(0xe8e4dc)
}
pub fn body() -> Hsla {
    c(0xb3aea5)
}
pub fn mute() -> Hsla {
    c(0x7d7870)
}
pub fn faint() -> Hsla {
    c(0x504c47)
}

// Meaning
pub fn frost() -> Hsla {
    c(0xa9c4d9)
}
pub fn add_fg() -> Hsla {
    c(0x8fc49a)
}
pub fn add_bg() -> Hsla {
    c(0x0b1a0f)
}
pub fn add_emph() -> Hsla {
    c(0x163d20)
}
pub fn del_fg() -> Hsla {
    c(0xe0706c)
}
pub fn del_bg() -> Hsla {
    c(0x1f0a0a)
}
pub fn del_emph() -> Hsla {
    c(0x44161a)
}
pub fn mod_fg() -> Hsla {
    c(0xd4a95e)
}

// Scrollbar
pub fn thumb() -> Hsla {
    c(0x666360)
}
pub fn thumb_hover() -> Hsla {
    c(0x8f8a82)
}
pub fn thumb_active() -> Hsla {
    c(0xa9c4d9)
}

// Syntax
pub fn syn_keyword() -> Hsla {
    c(0xc8c2b8)
}
pub fn syn_function() -> Hsla {
    c(0xe8e4dc)
}
pub fn syn_type() -> Hsla {
    c(0xb8c6d1)
}
pub fn syn_string() -> Hsla {
    c(0xa7b89a)
}
pub fn syn_number() -> Hsla {
    c(0xd4a95e)
}
pub fn syn_comment() -> Hsla {
    c(0x6b6760)
}
pub fn syn_punct() -> Hsla {
    c(0x8f8a82)
}
pub fn syn_attr() -> Hsla {
    c(0xc9b5a0)
}

// Type scale
pub const TEXT_MICRO: Pixels = px(11.);
pub const TEXT_CONTROL: Pixels = px(12.);
pub const TEXT_LIST: Pixels = px(13.);
pub const TEXT_CODE: Pixels = px(14.);
pub const TEXT_DISPLAY: Pixels = px(22.);

// Geometry
pub const ROW_LIST: Pixels = px(26.);
pub const ROW_CODE: Pixels = px(22.);
pub const HEADER_H: Pixels = px(36.);
pub const TITLEBAR_H: Pixels = px(34.);
pub const STATUS_H: Pixels = px(24.);
pub const CONTROL_H: Pixels = px(24.);
pub const SIDEBAR_W: f32 = 300.;
pub const SIDEBAR_MIN: f32 = 240.;
/// Dragging the sidebar narrower than this collapses it.
pub const SIDEBAR_SNAP: f32 = 170.;
/// Below this width the sidebar uses its compact layout.
pub const SIDEBAR_COMPACT: f32 = 300.;
pub const RADIUS: Pixels = px(3.);

/// Font families tried in order; first one installed wins.
pub const FONT_CANDIDATES: &[&str] = &[
    "JetBrains Mono",
    "JetBrainsMono Nerd Font Mono",
    "JetBrainsMono Nerd Font",
    "JetBrainsMonoNL Nerd Font Mono",
    // Fallbacks by platform.
    "Menlo",
    "Cascadia Mono",
    "Consolas",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];
