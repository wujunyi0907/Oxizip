//! Design tokens and shared widget styles for the Oxizip GUI.
//!
//! Colors come from [`Tokens`], resolved from the active iced `Theme`, so light
//! and dark modes stay consistent. Views must not hard-code colors.

use iced::theme::Palette;
use iced::widget::{
    button, checkbox, container, overlay::menu, pick_list, progress_bar, scrollable, text_input,
};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

use crate::prefs::{ColorAccent, ThemeMode};

pub const WINDOW_RADIUS: f32 = 12.0;
pub const CONTROL_RADIUS: f32 = 8.0;
pub const SMALL_RADIUS: f32 = 6.0;

#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub dark: bool,
    pub bg: Color,
    pub surface: Color,
    pub elevated: Color,
    pub border: Color,
    pub hover: Color,
    pub text: Color,
    pub text_2: Color,
    pub text_3: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub danger: Color,
}

fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

fn accent_for(accent: ColorAccent, dark: bool) -> Color {
    rgb(match (accent, dark) {
        (ColorAccent::BrandBlue, false) => 0x2F6FD6,
        (ColorAccent::BrandBlue, true) => 0x4C94F0,
        (ColorAccent::DeepNavy, false) => 0x1F4E8C,
        (ColorAccent::DeepNavy, true) => 0x5B8FD0,
        (ColorAccent::Teal, false) => 0x1783A3,
        (ColorAccent::Teal, true) => 0x3CBBD6,
    })
}

/// Whether the OS prefers a dark app theme.
fn system_is_dark() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "/v",
                "AppsUseLightTheme",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).contains("0x0"),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        std::env::var("GTK_THEME")
            .map(|v| v.to_ascii_lowercase().contains("dark"))
            .unwrap_or(false)
    }
}

/// Build the iced theme for the user's mode and accent preference.
pub fn build(mode: ThemeMode, accent: ColorAccent) -> Theme {
    let dark = match mode {
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::System => system_is_dark(),
    };
    let t = tokens_for(dark, accent);
    Theme::custom(
        if dark { "Oxizip Dark" } else { "Oxizip Light" }.to_string(),
        Palette {
            background: t.bg,
            text: t.text,
            primary: t.accent,
            success: rgb(if dark { 0x3CB884 } else { 0x2E9E6B }),
            danger: t.danger,
        },
    )
}

fn tokens_for(dark: bool, accent: ColorAccent) -> Tokens {
    let accent_c = accent_for(accent, dark);
    if dark {
        Tokens {
            dark,
            bg: rgb(0x111318),
            surface: rgb(0x171A21),
            elevated: rgb(0x1D212A),
            border: rgb(0x262B36),
            hover: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
            text: rgb(0xECEEF3),
            text_2: rgb(0xBCC1CE),
            text_3: rgb(0x8A90A2),
            accent: accent_c,
            accent_soft: with_alpha(accent_c, 0.20),
            danger: rgb(0xF06565),
        }
    } else {
        Tokens {
            dark,
            bg: rgb(0xFAFAFB),
            surface: rgb(0xF2F3F6),
            elevated: rgb(0xFFFFFF),
            border: rgb(0xE1E4EA),
            hover: Color::from_rgba(0.0, 0.0, 0.0, 0.05),
            text: rgb(0x14161B),
            text_2: rgb(0x474C5A),
            text_3: rgb(0x727887),
            accent: accent_c,
            accent_soft: with_alpha(accent_c, 0.12),
            danger: rgb(0xD64545),
        }
    }
}

/// Resolve tokens from a theme built by [`build`].
pub fn tokens(theme: &Theme) -> Tokens {
    let p = theme.palette();
    let dark = theme.extended_palette().is_dark;
    let mut t = tokens_for(dark, ColorAccent::BrandBlue);
    t.accent = p.primary;
    t.accent_soft = with_alpha(p.primary, if dark { 0.20 } else { 0.12 });
    t
}

pub fn text_primary(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(tokens(theme).text),
    }
}

pub fn text_secondary(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(tokens(theme).text_2),
    }
}

pub fn text_muted(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(tokens(theme).text_3),
    }
}

pub fn text_danger(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(tokens(theme).danger),
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

fn btn_base(bg: Option<Color>, fg: Color, border: Option<Color>, radius: f32) -> button::Style {
    button::Style {
        background: bg.map(Background::Color),
        text_color: fg,
        border: Border {
            color: border.unwrap_or(Color::TRANSPARENT),
            width: if border.is_some() { 1.0 } else { 0.0 },
            radius: radius.into(),
        },
        shadow: Shadow::default(),
    }
}

pub fn btn_primary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (bg, fg) = match status {
        button::Status::Active => (t.accent, Color::WHITE),
        button::Status::Hovered => (mix(t.accent, Color::WHITE, 0.12), Color::WHITE),
        button::Status::Pressed => (mix(t.accent, Color::BLACK, 0.14), Color::WHITE),
        button::Status::Disabled => (with_alpha(t.accent, 0.35), with_alpha(Color::WHITE, 0.8)),
    };
    btn_base(Some(bg), fg, None, CONTROL_RADIUS)
}

pub fn btn_secondary(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (bg, fg) = match status {
        button::Status::Active => (t.elevated, t.text),
        button::Status::Hovered => (mix(t.elevated, t.text, 0.05), t.text),
        button::Status::Pressed => (mix(t.elevated, t.text, 0.10), t.text),
        button::Status::Disabled => (t.surface, t.text_3),
    };
    btn_base(Some(bg), fg, Some(t.border), CONTROL_RADIUS)
}

pub fn btn_ghost(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (bg, fg) = match status {
        button::Status::Active => (None, t.text),
        button::Status::Hovered => (Some(t.hover), t.text),
        button::Status::Pressed => (Some(t.accent_soft), t.text),
        button::Status::Disabled => (None, t.text_3),
    };
    btn_base(bg, fg, None, SMALL_RADIUS)
}

pub fn btn_danger(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (bg, fg) = match status {
        button::Status::Active => (t.danger, Color::WHITE),
        button::Status::Hovered => (mix(t.danger, Color::WHITE, 0.12), Color::WHITE),
        button::Status::Pressed => (mix(t.danger, Color::BLACK, 0.14), Color::WHITE),
        button::Status::Disabled => (with_alpha(t.danger, 0.35), with_alpha(Color::WHITE, 0.8)),
    };
    btn_base(Some(bg), fg, None, CONTROL_RADIUS)
}

/// Menu-bar entries and dropdown rows.
pub fn btn_menu(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    // Pure black/white for maximum legibility in menus.
    let ink = if t.dark { Color::WHITE } else { Color::BLACK };
    let (bg, fg) = match status {
        button::Status::Active => (None, ink),
        button::Status::Hovered | button::Status::Pressed => (Some(t.accent_soft), ink),
        button::Status::Disabled => (None, mix(t.elevated, ink, 0.55)),
    };
    btn_base(bg, fg, None, SMALL_RADIUS)
}

/// Window frame (outermost rounded shell).
pub fn frame(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(t.bg)),
        border: Border {
            color: t.border,
            width: 1.0,
            radius: WINDOW_RADIUS.into(),
        },
        ..Default::default()
    }
}

pub fn caption_bar(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(t.surface)),
        border: Border {
            color: t.border,
            width: 0.0,
            radius: iced::border::Radius {
                top_left: WINDOW_RADIUS,
                top_right: WINDOW_RADIUS,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
        },
        ..Default::default()
    }
}

pub fn menu_bar(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(t.bg)),
        ..Default::default()
    }
}

/// Inset panel (lists, license text, tree).
pub fn panel(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(t.surface)),
        border: Border {
            color: t.border,
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        ..Default::default()
    }
}

/// Column header: quiet, no outline.
pub fn list_header(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(t.surface)),
        border: Border {
            color: t.border,
            width: 0.0,
            radius: CONTROL_RADIUS.into(),
        },
        ..Default::default()
    }
}

/// Hairline between rows inside a grouped panel.
pub fn divider(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens(theme).border)),
        ..Default::default()
    }
}

/// Floating surface: dropdowns, dialogs.
pub fn elevated(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(t.elevated)),
        border: Border {
            color: t.border,
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, if t.dark { 0.45 } else { 0.16 }),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        ..Default::default()
    }
}

pub fn scrim(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(Color::from_rgba(
            0.0,
            0.0,
            0.0,
            if t.dark { 0.55 } else { 0.30 },
        ))),
        ..Default::default()
    }
}

pub fn row_selected(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens(theme).accent_soft)),
        border: Border {
            radius: SMALL_RADIUS.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Home start screen: flat base with a faint accent wash at the top.
pub fn home_stage(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    let grad = iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
        .add_stop(0.0, t.bg)
        .add_stop(1.0, mix(t.bg, t.accent, if t.dark { 0.10 } else { 0.07 }));
    container::Style {
        background: Some(Background::Gradient(iced::Gradient::Linear(grad))),
        ..Default::default()
    }
}

/// Accent badge behind white start-screen glyphs.
pub fn icon_badge(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens(theme).accent)),
        border: Border {
            radius: 14.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Large start-screen tile.
pub fn home_tile(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (bg, border) = match status {
        button::Status::Active => (t.elevated, t.border),
        button::Status::Hovered => (t.elevated, t.accent),
        button::Status::Pressed => (t.accent_soft, t.accent),
        button::Status::Disabled => (t.surface, t.border),
    };
    let mut s = btn_base(Some(bg), t.text, Some(border), 10.0);
    if matches!(status, button::Status::Hovered) {
        s.shadow = Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, if t.dark { 0.35 } else { 0.10 }),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        };
    }
    s
}

pub fn btn_caption_close(theme: &Theme, status: button::Status) -> button::Style {
    let t = tokens(theme);
    let (bg, fg) = match status {
        button::Status::Hovered => (Some(t.danger), Color::WHITE),
        button::Status::Pressed => (Some(mix(t.danger, Color::BLACK, 0.14)), Color::WHITE),
        button::Status::Active => (None, t.text_2),
        button::Status::Disabled => (None, t.text_3),
    };
    btn_base(bg, fg, None, SMALL_RADIUS)
}

pub fn status_bar(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(t.surface)),
        border: Border {
            color: t.border,
            width: 1.0,
            radius: iced::border::Radius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: WINDOW_RADIUS,
                bottom_left: WINDOW_RADIUS,
            },
        },
        ..Default::default()
    }
}

pub fn error_banner(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(with_alpha(
            t.danger,
            if t.dark { 0.16 } else { 0.08 },
        ))),
        border: Border {
            color: with_alpha(t.danger, 0.45),
            width: 1.0,
            radius: SMALL_RADIUS.into(),
        },
        ..Default::default()
    }
}

pub fn pill(theme: &Theme, accent: bool) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(Background::Color(if accent {
            t.accent_soft
        } else {
            t.elevated
        })),
        border: Border {
            color: if accent {
                with_alpha(t.accent, 0.35)
            } else {
                t.border
            },
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    }
}

pub fn input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let t = tokens(theme);
    let border_color = match status {
        text_input::Status::Active => t.border,
        text_input::Status::Hovered => mix(t.border, t.text, 0.25),
        text_input::Status::Focused => t.accent,
        text_input::Status::Disabled => t.border,
    };
    let bg = match status {
        text_input::Status::Disabled => t.surface,
        _ => t.elevated,
    };
    text_input::Style {
        background: Background::Color(bg),
        border: Border {
            color: border_color,
            width: if matches!(status, text_input::Status::Focused) {
                1.5
            } else {
                1.0
            },
            radius: CONTROL_RADIUS.into(),
        },
        icon: t.text_3,
        placeholder: t.text_3,
        value: if matches!(status, text_input::Status::Disabled) {
            t.text_3
        } else {
            t.text
        },
        selection: t.accent_soft,
    }
}

pub fn pick_list_style(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let t = tokens(theme);
    let border_color = match status {
        pick_list::Status::Active => t.border,
        pick_list::Status::Hovered => mix(t.border, t.text, 0.25),
        pick_list::Status::Opened => t.accent,
    };
    pick_list::Style {
        text_color: t.text,
        placeholder_color: t.text_3,
        handle_color: t.text_2,
        background: Background::Color(t.elevated),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
    }
}

pub fn pick_menu_style(theme: &Theme) -> menu::Style {
    let t = tokens(theme);
    menu::Style {
        background: Background::Color(t.elevated),
        border: Border {
            color: t.border,
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        text_color: t.text,
        selected_text_color: Color::WHITE,
        selected_background: Background::Color(t.accent),
    }
}

pub fn checkbox_style(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let t = tokens(theme);
    let (is_checked, hovered, disabled) = match status {
        checkbox::Status::Active { is_checked } => (is_checked, false, false),
        checkbox::Status::Hovered { is_checked } => (is_checked, true, false),
        checkbox::Status::Disabled { is_checked } => (is_checked, false, true),
    };
    let bg = if disabled {
        t.surface
    } else if is_checked {
        if hovered {
            mix(t.accent, Color::WHITE, 0.12)
        } else {
            t.accent
        }
    } else if hovered {
        t.hover
    } else {
        t.elevated
    };
    let border_color = if disabled {
        t.border
    } else if is_checked {
        t.accent
    } else if hovered {
        mix(t.border, t.accent, 0.5)
    } else {
        t.border
    };
    checkbox::Style {
        background: Background::Color(bg),
        icon_color: Color::WHITE,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        text_color: Some(if disabled { t.text_3 } else { t.text }),
    }
}

pub fn progress_style(theme: &Theme) -> progress_bar::Style {
    let t = tokens(theme);
    progress_bar::Style {
        background: Background::Color(t.surface),
        bar: Background::Color(t.accent),
        border: Border {
            color: t.border,
            width: 0.5,
            radius: 4.0.into(),
        },
    }
}

pub fn scrollable_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let t = tokens(theme);
    let hovered = matches!(
        status,
        scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. }
    );
    let scroller_color = if hovered {
        with_alpha(t.text_3, 0.65)
    } else {
        with_alpha(t.text_3, 0.35)
    };
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            color: scroller_color,
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
    }
}
