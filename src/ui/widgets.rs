//! Small shared primitives (docs/03-style.md §5). New pattern used twice → lives here.

use crate::git::ChangeStatus;
use crate::theme;
use gpui::{div, prelude::*, px, Div, Hsla, SharedString, Stateful};

/// Title Case section label: `Base`, `Repository`.
pub fn micro(text: impl Into<SharedString>) -> Div {
    div().text_size(theme::TEXT_MICRO).text_color(theme::mute()).child(text.into())
}

pub fn status_color(s: ChangeStatus) -> Hsla {
    match s {
        ChangeStatus::Added => theme::add_fg(),
        ChangeStatus::Deleted => theme::del_fg(),
        ChangeStatus::Modified | ChangeStatus::TypeChange => theme::mod_fg(),
        ChangeStatus::Renamed | ChangeStatus::Copied => theme::frost(),
    }
}

pub fn status_glyph(s: ChangeStatus) -> Div {
    div()
        .w(px(12.))
        .flex_none()
        .text_size(theme::TEXT_CONTROL)
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(status_color(s))
        .child(s.glyph())
}

/// `+42 −8` counts; `None` renders nothing for that side.
pub fn counts(add: Option<u64>, del: Option<u64>) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(6.))
        .text_size(theme::TEXT_CONTROL)
        .when_some(add.filter(|a| *a > 0), |d, a| d.child(div().text_color(theme::add_fg()).child(format!("+{a}"))))
        .when_some(del.filter(|x| *x > 0), |d, x| d.child(div().text_color(theme::del_fg()).child(format!("−{x}"))))
}

/// Segmented icon toggle cell.
pub fn seg(id: impl Into<SharedString>, label: &'static str, active: bool, tooltip: &'static str) -> Stateful<Div> {
    div()
        .id(gpui::ElementId::Name(id.into()))
        .h(theme::CONTROL_H)
        .min_w(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme::RADIUS)
        .text_size(theme::TEXT_CONTROL)
        .cursor_pointer()
        .text_color(if active { theme::bone() } else { theme::mute() })
        .when(active, |d| d.bg(theme::slate()))
        .when(!active, |d| d.hover(|s| s.bg(theme::ash()).text_color(theme::body())))
        .tooltip(move |_, cx| cx.new(|_| Tip(tooltip)).into())
        .child(label)
}

/// Plain text tooltip.
pub struct Tip(pub &'static str);

impl Render for Tip {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .bg(theme::crypt())
            .border_1()
            .border_color(theme::line_hi())
            .rounded(theme::RADIUS)
            .text_size(theme::TEXT_CONTROL)
            .text_color(theme::body())
            .child(self.0)
    }
}

pub fn spinner() -> Div {
    div().text_size(theme::TEXT_LIST).text_color(theme::mute()).child("•••")
}

/// "3d", "5h", "just now"
pub fn age(ts: i64) -> String {
    let now =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(ts);
    let s = (now - ts).max(0);
    match s {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        86_400..=2_591_999 => format!("{}d", s / 86_400),
        2_592_000..=31_535_999 => format!("{}mo", s / 2_592_000),
        _ => format!("{}y", s / 31_536_000),
    }
}

/// Human byte size.
pub fn bytes(n: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024. && u < U.len() - 1 {
        v /= 1024.;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[u])
    }
}

/// Thousands separator.
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Icon for a ref kind. Nerd Font glyphs when the Nerd variant is the UI font, else letters.
pub fn ref_icon(look: super::app::RefLook, nerd: bool) -> &'static str {
    use super::app::RefLook::*;
    match (look, nerd) {
        (Branch, true) => "\u{e0a0}",
        (Remote, true) => "\u{f0c2}",
        (Tag, true) => "\u{f02b}",
        (Commit, true) => "\u{f417}",
        (Rev, true) => "\u{f1da}",
        (Branch, false) => "br",
        (Remote, false) => "rm",
        (Tag, false) => "tg",
        (Commit, false) => "#",
        (Rev, false) => "@",
    }
}

pub fn ref_icon_color(look: super::app::RefLook) -> Hsla {
    use super::app::RefLook::*;
    match look {
        Branch => theme::add_fg(),
        Remote => theme::frost(),
        Tag => theme::mod_fg(),
        Commit => theme::syn_type(),
        Rev => theme::mute(),
    }
}

/// Disclosure chevron. Nerd Font codicons (crisp, Zed-like) or large triangles as fallback.
pub fn chevron(open: bool, nerd: bool) -> Div {
    let glyph = match (open, nerd) {
        (true, true) => "\u{eab4}",
        (false, true) => "\u{eab6}",
        (true, false) => "▼",
        (false, false) => "▶",
    };
    div()
        .w(px(16.))
        .flex_none()
        .flex()
        .justify_center()
        .text_size(if nerd { theme::TEXT_LIST } else { theme::TEXT_MICRO })
        .text_color(theme::mute())
        .child(glyph)
}

/// The Kerf mark (B2 "hairline kerf") drawn with plain shapes, crisp at any size.
/// Mirrors `examples/icon.rs`: small sizes get a thicker hairline so the cut stays visible.
pub fn app_icon(size: f32) -> Div {
    let (slit, bar, gap) = if size <= 20. {
        (7.0, 25.0, 3.0)
    } else if size <= 40. {
        (4.0, 24.5, 1.5)
    } else if size <= 72. {
        (2.4, 23.0, 2.0)
    } else {
        (1.5, 22.0, 2.25)
    };
    let s = size / 120.;
    let rect = |x: f32, y: f32, w: f32, h: f32, color: Hsla, r: f32| {
        div().absolute().left(px(x * s)).top(px(y * s)).w(px(w * s)).h(px(h * s)).rounded(px(r * s)).bg(color)
    };
    let cx = 60.;
    div()
        .relative()
        .flex_none()
        .w(px(size))
        .h(px(size))
        .rounded(px(28. * s))
        .bg(theme::void())
        .border_1()
        .border_color(theme::line_hi())
        .child(rect(cx - slit / 2. - gap - bar, 28., bar, 58., theme::del_fg(), 3.))
        .child(rect(cx + slit / 2. + gap, 38., bar, 58., theme::add_fg(), 3.))
        .child(rect(cx - slit / 2., 20., slit, 84., theme::frost(), 0.))
}
