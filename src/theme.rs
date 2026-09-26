//! Themes. Every colour used by the UI comes from the active [`Palette`] (docs/03-style.md);
//! sizes are shared. Switching theme swaps the palette and repaints every window.

use gpui::{px, rgb, Hsla, Pixels};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    BlackMetal,
    GruvboxDark,
    GruvboxLight,
    EverforestLight,
}

impl ThemeId {
    pub const ALL: [ThemeId; 4] =
        [ThemeId::BlackMetal, ThemeId::GruvboxDark, ThemeId::GruvboxLight, ThemeId::EverforestLight];

    pub fn name(self) -> &'static str {
        match self {
            ThemeId::BlackMetal => "Black Metal",
            ThemeId::GruvboxDark => "Gruvbox Dark",
            ThemeId::GruvboxLight => "Gruvbox Light",
            ThemeId::EverforestLight => "Everforest Light",
        }
    }
    /// Stable id for the settings file.
    pub fn key(self) -> &'static str {
        match self {
            ThemeId::BlackMetal => "black-metal",
            ThemeId::GruvboxDark => "gruvbox-dark",
            ThemeId::GruvboxLight => "gruvbox-light",
            ThemeId::EverforestLight => "everforest-light",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }
    pub fn is_dark(self) -> bool {
        matches!(self, ThemeId::BlackMetal | ThemeId::GruvboxDark)
    }
}

/// All colour tokens. Surfaces go darkest → lightest in dark themes (void is the editor
/// canvas); light themes keep the same roles with the editor as the calmest surface.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub void: u32,
    pub abyss: u32,
    pub crypt: u32,
    pub ash: u32,
    pub slate: u32,
    pub line: u32,
    pub line_hi: u32,
    pub bone: u32,
    pub body: u32,
    pub mute: u32,
    pub faint: u32,
    pub frost: u32,
    pub add_fg: u32,
    pub add_bg: u32,
    pub add_emph: u32,
    pub del_fg: u32,
    pub del_bg: u32,
    pub del_emph: u32,
    pub mod_fg: u32,
    pub thumb: u32,
    pub thumb_hover: u32,
    pub syn_keyword: u32,
    pub syn_function: u32,
    pub syn_type: u32,
    pub syn_string: u32,
    pub syn_number: u32,
    pub syn_comment: u32,
    pub syn_punct: u32,
    pub syn_attr: u32,
}

/// Black Metal — the original: pure black, bone type, one frost accent.
const BLACK_METAL: Palette = Palette {
    void: 0x000000,
    abyss: 0x0a0a0a,
    crypt: 0x111111,
    ash: 0x1a1a1a,
    slate: 0x222222,
    line: 0x1c1c1c,
    line_hi: 0x2e2e2e,
    bone: 0xe8e4dc,
    body: 0xb3aea5,
    mute: 0x7d7870,
    faint: 0x504c47,
    frost: 0xa9c4d9,
    add_fg: 0x8fc49a,
    add_bg: 0x0b1a0f,
    add_emph: 0x163d20,
    del_fg: 0xe0706c,
    del_bg: 0x1f0a0a,
    del_emph: 0x44161a,
    mod_fg: 0xd4a95e,
    thumb: 0x666360,
    thumb_hover: 0x8f8a82,
    syn_keyword: 0xc8c2b8,
    syn_function: 0xe8e4dc,
    syn_type: 0xb8c6d1,
    syn_string: 0xa7b89a,
    syn_number: 0xd4a95e,
    syn_comment: 0x6b6760,
    syn_punct: 0x8f8a82,
    syn_attr: 0xc9b5a0,
};

/// Gruvbox Dark (hard) — warm retro palette.
const GRUVBOX_DARK: Palette = Palette {
    void: 0x1d2021,
    abyss: 0x282828,
    crypt: 0x32302f,
    ash: 0x3c3836,
    slate: 0x504945,
    line: 0x3c3836,
    line_hi: 0x504945,
    bone: 0xebdbb2,
    body: 0xd5c4a1,
    mute: 0xa89984,
    faint: 0x7c6f64,
    frost: 0x83a598,
    add_fg: 0xb8bb26,
    add_bg: 0x2a2e1c,
    add_emph: 0x3d4220,
    del_fg: 0xfb4934,
    del_bg: 0x321a18,
    del_emph: 0x5a2622,
    mod_fg: 0xfabd2f,
    thumb: 0x665c54,
    thumb_hover: 0x7c6f64,
    syn_keyword: 0xfb4934,
    syn_function: 0x8ec07c,
    syn_type: 0xfabd2f,
    syn_string: 0xb8bb26,
    syn_number: 0xd3869b,
    syn_comment: 0x928374,
    syn_punct: 0xa89984,
    syn_attr: 0x8ec07c,
};

/// Gruvbox Light (soft) — warm cream, not bright white. Text tokens darkened for 4.5:1.
const GRUVBOX_LIGHT: Palette = Palette {
    void: 0xf2e5bc,
    abyss: 0xebdbb2,
    crypt: 0xe6d6ab,
    ash: 0xdccca2,
    slate: 0xd5c4a1,
    line: 0xd5c4a1,
    line_hi: 0xbdae93,
    bone: 0x282828,
    body: 0x3c3836,
    mute: 0x5a524c,
    faint: 0x928374,
    frost: 0x076678,
    add_fg: 0x5f5a08,
    add_bg: 0xe6e1b0,
    add_emph: 0xd3d08c,
    del_fg: 0x9d0006,
    del_bg: 0xf0d4bc,
    del_emph: 0xe6b9a0,
    mod_fg: 0x8f5a0a,
    thumb: 0xa89984,
    thumb_hover: 0x7c6f64,
    syn_keyword: 0x9d0006,
    syn_function: 0x427b58,
    syn_type: 0x8f5a0a,
    syn_string: 0x5f5a08,
    syn_number: 0x8f3f71,
    syn_comment: 0x6f6358,
    syn_punct: 0x5a524c,
    syn_attr: 0x427b58,
};

/// Everforest Light (medium) — muted sage-cream, low glare. Text tokens darkened for 4.5:1.
const EVERFOREST_LIGHT: Palette = Palette {
    void: 0xefebd4,
    abyss: 0xe8e4cc,
    crypt: 0xe2dec5,
    ash: 0xd9d5bd,
    slate: 0xcfcbb3,
    line: 0xdcd8c0,
    line_hi: 0xbdc3af,
    bone: 0x2f383d,
    body: 0x3f4a50,
    mute: 0x56635b,
    faint: 0x939f91,
    frost: 0x2a668f,
    add_fg: 0x546400,
    add_bg: 0xe2e6c6,
    add_emph: 0xcdd6a3,
    del_fg: 0xb53633,
    del_bg: 0xf3ddd4,
    del_emph: 0xecc2b5,
    mod_fg: 0x8a6400,
    thumb: 0xa6b0a0,
    thumb_hover: 0x829181,
    syn_keyword: 0xb53633,
    syn_function: 0x2c7a5c,
    syn_type: 0x8a6400,
    syn_string: 0x546400,
    syn_number: 0xa14784,
    syn_comment: 0x5f6c61,
    syn_punct: 0x56635b,
    syn_attr: 0x2c7a5c,
};

pub fn palette_of(id: ThemeId) -> &'static Palette {
    match id {
        ThemeId::BlackMetal => &BLACK_METAL,
        ThemeId::GruvboxDark => &GRUVBOX_DARK,
        ThemeId::GruvboxLight => &GRUVBOX_LIGHT,
        ThemeId::EverforestLight => &EVERFOREST_LIGHT,
    }
}

static CURRENT: AtomicUsize = AtomicUsize::new(0);

pub fn current() -> ThemeId {
    ThemeId::ALL[CURRENT.load(Ordering::Relaxed).min(ThemeId::ALL.len() - 1)]
}

/// Switch the active theme (callers repaint windows afterwards).
pub fn set_current(id: ThemeId) {
    let ix = ThemeId::ALL.iter().position(|t| *t == id).unwrap_or(0);
    CURRENT.store(ix, Ordering::Relaxed);
}

fn p() -> &'static Palette {
    // Keep the lookup trivially cheap: these run for every element every frame.
    palette_of(current())
}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

macro_rules! tokens {
    ($($name:ident),* $(,)?) => {
        $(pub fn $name() -> Hsla { c(p().$name) })*
    };
}

tokens!(
    void,
    abyss,
    crypt,
    ash,
    slate,
    line,
    line_hi,
    bone,
    body,
    mute,
    faint,
    frost,
    add_fg,
    add_bg,
    add_emph,
    del_fg,
    del_bg,
    del_emph,
    mod_fg,
    thumb,
    thumb_hover,
    syn_keyword,
    syn_function,
    syn_type,
    syn_string,
    syn_number,
    syn_comment,
    syn_punct,
    syn_attr,
);

/// Dragged scrollbar thumb: the theme accent.
pub fn thumb_active() -> Hsla {
    frost()
}

/// Translucent wash over content (minimap viewport frame): white on dark, black on light.
pub fn wash(alpha: f32) -> Hsla {
    if current().is_dark() {
        gpui::hsla(0., 0., 1., alpha)
    } else {
        gpui::hsla(0., 0., 0., alpha * 0.8)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(hex: u32) -> f64 {
        let ch = |v: u32| {
            let c = v as f64 / 255.;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch((hex >> 16) & 0xff) + 0.7152 * ch((hex >> 8) & 0xff) + 0.0722 * ch(hex & 0xff)
    }

    fn contrast(a: u32, b: u32) -> f64 {
        let (a, b) = (lum(a), lum(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Every theme keeps readable text (WCAG AA 4.5:1; comments ≥ 4.4 as decorative).
    #[test]
    fn every_theme_meets_contrast() {
        for id in ThemeId::ALL {
            let p = palette_of(id);
            let checks = [
                ("bone/void", p.bone, p.void, 4.5),
                ("body/void", p.body, p.void, 4.5),
                ("body/abyss", p.body, p.abyss, 4.5),
                ("mute/void", p.mute, p.void, 4.5),
                ("mute/abyss", p.mute, p.abyss, 4.4),
                ("add_fg/add_bg", p.add_fg, p.add_bg, 4.5),
                ("del_fg/del_bg", p.del_fg, p.del_bg, 4.5),
                ("bone/add_emph", p.bone, p.add_emph, 4.5),
                ("bone/del_emph", p.bone, p.del_emph, 4.5),
                ("frost/void", p.frost, p.void, 3.0),
                ("comment/void", p.syn_comment, p.void, 3.5),
            ];
            for (name, fg, bg, min) in checks {
                let c = contrast(fg, bg);
                assert!(c >= min, "{}: {name} = {c:.2} < {min}", id.name());
            }
        }
    }

    #[test]
    fn theme_keys_round_trip() {
        for id in ThemeId::ALL {
            assert_eq!(ThemeId::from_key(id.key()), Some(id));
        }
        assert_eq!(ThemeId::from_key("nope"), None);
    }
}
