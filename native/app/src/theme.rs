//! Colors and small style helpers shared by the views.
//!
//! Dark theme of the "prismux · Shepherd" mockup: near-black surfaces,
//! cyan accent, hairline borders and small rounded corners (4-6 px).

use iced::widget::button;
use iced::{font, Background, Border, Color, Font, Padding, Theme};
use shepherd_app_core::snapshot::Status;

/// A colour of the dark themes, or its twin on the light one.
pub fn pick(dark: Color, light: Color) -> Color {
    if crate::appearance::is_light() {
        light
    } else {
        dark
    }
}

// Palette of the "prismux · Shepherd" design (CSS custom properties of the mockup).
/// --bg
pub fn bg() -> Color {
    pick(
        Color::from_rgb8(0x07, 0x07, 0x0A),
        Color::from_rgb8(0xF6, 0xF6, 0xF8),
    )
}
/// --panel
pub fn panel() -> Color {
    pick(
        Color::from_rgb8(0x0B, 0x0B, 0x10),
        Color::from_rgb8(0xFF, 0xFF, 0xFF),
    )
}
/// Top bar, rail and status bar (#09090D).
pub fn bar() -> Color {
    pick(
        Color::from_rgb8(0x09, 0x09, 0x0D),
        Color::from_rgb8(0xEE, 0xEE, 0xF2),
    )
}
/// --panel2: cards, hover.
pub fn surface() -> Color {
    pick(
        Color::from_rgb8(0x10, 0x10, 0x17),
        Color::from_rgb8(0xF1, 0xF1, 0xF5),
    )
}
/// --panel3: selected items.
pub fn selected() -> Color {
    pick(
        Color::from_rgb8(0x15, 0x15, 0x1C),
        Color::from_rgb8(0xE4, 0xE4, 0xEC),
    )
}
/// --line: hairline separators.
pub fn line() -> Color {
    pick(
        Color::from_rgb8(0x1E, 0x1E, 0x27),
        Color::from_rgb8(0xDC, 0xDC, 0xE4),
    )
}
/// --line2: borders of controls.
pub fn line2() -> Color {
    pick(
        Color::from_rgb8(0x2C, 0x2C, 0x38),
        Color::from_rgb8(0xC8, 0xC8, 0xD4),
    )
}
/// --text
pub fn fg() -> Color {
    pick(
        Color::from_rgb8(0xE7, 0xE7, 0xEE),
        Color::from_rgb8(0x1C, 0x1C, 0x24),
    )
}
/// --hi: titles and emphasis.
pub fn hi() -> Color {
    pick(
        Color::from_rgb8(0xF4, 0xF4, 0xF8),
        Color::from_rgb8(0x0A, 0x0A, 0x10),
    )
}
/// --muted
pub fn dim() -> Color {
    pick(
        Color::from_rgb8(0x9A, 0x9A, 0xA8),
        Color::from_rgb8(0x5C, 0x5C, 0x6C),
    )
}
/// Hovered rows and items (`hover`).
pub fn hover() -> Color {
    pick(
        Color::from_rgb8(0x0F, 0x0F, 0x15),
        Color::from_rgb8(0xEC, 0xEC, 0xF2),
    )
}
/// Input and code backgrounds (`input`).
pub fn input() -> Color {
    pick(
        Color::from_rgb8(0x05, 0x05, 0x07),
        Color::from_rgb8(0xFF, 0xFF, 0xFF),
    )
}
/// Hovered control borders (`line3`).
pub fn line3() -> Color {
    pick(
        Color::from_rgb8(0x4A, 0x4A, 0x5C),
        Color::from_rgb8(0x9A, 0x9A, 0xAC),
    )
}
/// Secondary text that must stay readable (`text2`).
pub fn text2() -> Color {
    pick(
        Color::from_rgb8(0xC9, 0xC9, 0xD3),
        Color::from_rgb8(0x3A, 0x3A, 0x48),
    )
}
/// Decorative only: separators, keyboard hints, disabled (`faint`). Never for information.
pub fn faint() -> Color {
    pick(
        Color::from_rgb8(0x6E, 0x6E, 0x7C),
        Color::from_rgb8(0x8A, 0x8A, 0x98),
    )
}
/// --cyan: accent / working.
pub fn accent() -> Color {
    pick(
        Color::from_rgb8(0x19, 0xD3, 0xFF),
        Color::from_rgb8(0x0A, 0x8F, 0xB8),
    )
}
/// --red
pub fn bad() -> Color {
    pick(
        Color::from_rgb8(0xFF, 0x6B, 0x8A),
        Color::from_rgb8(0xD6, 0x33, 0x5A),
    )
}
/// --green
pub fn ok() -> Color {
    pick(
        Color::from_rgb8(0x7C, 0xF2, 0x9A),
        Color::from_rgb8(0x1F, 0x9D, 0x4A),
    )
}
/// --orange: blocked / needs you.
pub fn warn() -> Color {
    pick(
        Color::from_rgb8(0xFF, 0x8A, 0x1F),
        Color::from_rgb8(0xD9, 0x68, 0x0A),
    )
}
/// --violet
#[allow(dead_code)] // used by the views of phase 3
pub fn violet() -> Color {
    pick(
        Color::from_rgb8(0xB9, 0x8C, 0xFF),
        Color::from_rgb8(0x7C, 0x4D, 0xD6),
    )
}
/// --idle
pub fn idle() -> Color {
    pick(
        Color::from_rgb8(0xB4, 0xB4, 0xC0),
        Color::from_rgb8(0x6A, 0x6A, 0x78),
    )
}
/// Text over bright fills: dark on the dark themes, white on the light one.
fn on_bright() -> Color {
    pick(Color::from_rgb8(0x07, 0x07, 0x0A), Color::WHITE)
}

/// Body font (`font-family: var(--mono)`); the default font of the app.
pub const MONO: Font = Font::with_name("JetBrains Mono");
#[allow(dead_code)] // used by the views of phase 3
pub const MONO_BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..MONO
};
/// Pixel font for brand and section labels (`--pixel`).
pub const PIXEL: Font = Font::with_name("Silkscreen");
pub const PIXEL_BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..PIXEL
};

/// Fonts bundled with the app (SIL OFL, licenses next to the files).
pub const FONT_FILES: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../assets/fonts/Silkscreen-Regular.ttf"),
    include_bytes!("../assets/fonts/Silkscreen-Bold.ttf"),
];

pub fn card_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(surface())),
        border: Border {
            color: line(),
            width: 1.0,
            radius: 6.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

pub fn surface_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(bar())),
        border: Border {
            color: line(),
            width: 0.5,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// Padding with a value per side (iced only converts 1 or 2 values).
pub fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding {
        top,
        right,
        bottom,
        left,
    }
}

pub fn status_glyph(status: Status) -> &'static str {
    match status {
        Status::Working => "▶",
        Status::Blocked => "!",
        Status::Done => "✓",
        Status::Idle => "–",
        Status::Unknown => "·",
    }
}

pub fn status_name(status: Status) -> &'static str {
    match status {
        Status::Working => "working",
        Status::Blocked => "blocked",
        Status::Done => "finished",
        Status::Idle => "idle",
        Status::Unknown => "Other",
    }
}

pub fn status_color(status: Status) -> Color {
    match status {
        Status::Working => accent(),
        Status::Blocked => warn(),
        Status::Done => ok(),
        Status::Idle | Status::Unknown => idle(),
    }
}

fn hovered(status: button::Status) -> bool {
    matches!(status, button::Status::Hovered | button::Status::Pressed)
}

fn rounded(radius: f32) -> Border {
    Border {
        radius: radius.into(),
        ..Border::default()
    }
}

/// Sidebar items, tabs and mode buttons (`.tab`, `.ws`, `.ag`): selected lifts to
/// `--panel3` with a `--line2` ring; hover lifts to `--panel2`.
pub fn item_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (background, border) = if selected {
            (
                Some(Background::Color(self::selected())),
                Border {
                    color: line2(),
                    width: 1.0,
                    radius: 4.0.into(),
                },
            )
        } else if hovered(status) {
            (Some(Background::Color(hover())), rounded(4.0))
        } else {
            (None, rounded(4.0))
        };
        button::Style {
            background,
            text_color: if selected { hi() } else { fg() },
            border,
            ..button::Style::default()
        }
    }
}

/// Close buttons (✕): minimal, red tint on hover.
#[allow(dead_code)]
pub fn close_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = hovered(status);
        button::Style {
            background: hovered.then_some(Background::Color(Color { a: 0.12, ..bad() })),
            text_color: if hovered { bad() } else { dim() },
            border: rounded(4.0),
            ..button::Style::default()
        }
    }
}

/// `.btn.pri`: bright fill, dark bold text.
pub fn primary_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let background = if hovered(status) { fg() } else { hi() };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: on_bright(),
            border: rounded(4.0),
            ..button::Style::default()
        }
    }
}

/// `.btn.warn`: orange fill, dark text.
#[allow(dead_code)] // used by the views of phase 3
pub fn warn_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let background = if hovered(status) {
            Color::from_rgb8(0xFF, 0xA0, 0x4A)
        } else {
            warn()
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: on_bright(),
            border: rounded(4.0),
            ..button::Style::default()
        }
    }
}

/// Destructive actions ("Close agent", "Delete"): red fill, dark text.
pub fn danger_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let background = if hovered(status) {
            Color::from_rgb8(0xFF, 0x8C, 0xA4)
        } else {
            bad()
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: on_bright(),
            border: rounded(4.0),
            ..button::Style::default()
        }
    }
}

/// Destructive action before its confirmation: red outline, fills red once armed (`danger_button_style`).
pub fn destructive_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = hovered(status);
        button::Style {
            background: hovered.then_some(Background::Color(Color { a: 0.12, ..bad() })),
            text_color: bad(),
            border: Border {
                color: if hovered {
                    bad()
                } else {
                    Color { a: 0.55, ..bad() }
                },
                width: 1.0,
                radius: 4.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// A filter chip (`All`, `Running`, ...): outlined pill, cyan when it is the active one.
pub fn chip_style(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = hovered(status);
        button::Style {
            background: active.then_some(Background::Color(selected())),
            text_color: if active {
                hi()
            } else if hovered {
                fg()
            } else {
                dim()
            },
            border: Border {
                color: if active { accent() } else { line2() },
                width: 1.0,
                radius: 14.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// A toast: a card with a coloured edge (green for success, red for a server refusal).
pub fn toast_style(color: Color) -> impl Fn(&Theme) -> iced::widget::container::Style {
    move |_| iced::widget::container::Style {
        background: Some(Background::Color(bar())),
        border: Border {
            color,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// `.btn` (outlined) and `.btn.ghost` (no border, `ghost = true`).
#[allow(dead_code)] // used by the views of phase 3
pub fn outline_button_style(ghost: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = hovered(status);
        button::Style {
            background: (ghost && hovered).then_some(Background::Color(surface())),
            text_color: if ghost && !hovered { dim() } else { fg() },
            border: Border {
                color: if ghost {
                    Color::TRANSPARENT
                } else if hovered {
                    dim()
                } else {
                    line2()
                },
                width: 1.0,
                radius: 4.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// `.btn.link`: cyan text, no border.
#[allow(dead_code)] // used by the views of phase 3
pub fn link_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| button::Style {
        background: hovered(status).then_some(Background::Color(surface())),
        text_color: accent(),
        border: rounded(4.0),
        ..button::Style::default()
    }
}

/// Small "mini" buttons: no background, subtle hover.
pub fn mini_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = hovered(status);
        button::Style {
            background: hovered.then_some(Background::Color(surface())),
            text_color: if hovered { fg() } else { dim() },
            border: rounded(4.0),
            ..button::Style::default()
        }
    }
}

/// A button with no box: transparent, a faint fill on hover (sidebar rows, "+" and the chevrons).
pub fn ghost_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| button::Style {
        background: hovered(status).then_some(Background::Color(hover())),
        text_color: fg(),
        border: rounded(5.0),
        ..button::Style::default()
    }
}

/// `.badge`: small solid pill with dark bold text.
#[allow(dead_code)] // used by the views of phase 3
pub fn badge_style(color: Color) -> impl Fn(&Theme) -> iced::widget::container::Style {
    move |_| iced::widget::container::Style {
        background: Some(Background::Color(color)),
        text_color: Some(on_bright()),
        border: rounded(3.0),
        ..iced::widget::container::Style::default()
    }
}

/// `.kbd`: key cap with a thicker bottom edge is approximated by a 1 px outline.
#[allow(dead_code)] // used by the layout of phase 2
pub fn kbd_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        text_color: Some(fg()),
        border: Border {
            color: line2(),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// `.alert`: tinted box with a 50% border (`color` = orange for warnings, red for errors).
#[allow(dead_code)] // used by the views of phase 3
pub fn alert_style(color: Color) -> impl Fn(&Theme) -> iced::widget::container::Style {
    move |_| iced::widget::container::Style {
        background: Some(Background::Color(Color { a: 0.06, ..color })),
        border: Border {
            color: Color { a: 0.5, ..color },
            width: 1.0,
            radius: 6.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// Section / KPI label (`.lbl`, `.k`): pixel font, small, muted.
pub fn label<'a>(content: impl iced::widget::text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    iced::widget::text(content)
        .font(PIXEL)
        .size(10.5)
        .color(dim())
}

/// `.kpis`: cells separated by 1 px lines (the strip background shows through the row spacing).
pub fn kpi_strip_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(line())),
        border: Border {
            color: line(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// `.kpi`: one cell of the strip.
pub fn kpi_cell_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(panel())),
        ..iced::widget::container::Style::default()
    }
}

/// A bar with a 1 px line along its bottom edge (`border-bottom: 1px solid var(--line)`).
/// A bar with a hairline on its top edge (the composer).
pub fn top_line_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(bar())),
        border: Border {
            color: line(),
            width: 0.5,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

pub fn bottom_line_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(panel())),
        border: Border {
            color: line(),
            width: 0.5,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// `.tbar` / `.sbar`: darker bar with a hairline border.
pub fn bar_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(bar())),
        border: Border {
            color: line(),
            width: 0.5,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// `.inp`: dark field with a `--line2` border; cyan border while focused.
pub fn input_style(
    _: &Theme,
    status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    let focused = matches!(status, iced::widget::text_input::Status::Focused { .. });
    iced::widget::text_input::Style {
        background: Background::Color(input()),
        border: Border {
            color: if focused { accent() } else { line2() },
            width: 1.0,
            radius: 6.0.into(),
        },
        icon: dim(),
        placeholder: dim(),
        value: fg(),
        selection: Color { a: 0.3, ..accent() },
    }
}

/// A small box on a card: surface fill, hairline border (publisher chips, consumer group cards).
pub fn chip_box_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(surface())),
        border: Border {
            color: line(),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// The input style with the red outline of a field that the form refused to send (3.6 of the spec).
pub fn input_style_marked(
    refused: bool,
) -> impl Fn(&Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |theme, status| {
        let mut style = input_style(theme, status);
        if refused {
            style.border.color = bad();
        }
        style
    }
}

/// `.graph`: the pattern canvas, darker than the panels, with a hairline border.
pub fn graph_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(pick(
            Color::from_rgb8(0x08, 0x08, 0x0C),
            Color::from_rgb8(0xF3, 0xF3, 0xF7),
        ))),
        border: Border {
            color: line(),
            width: 1.0,
            radius: 6.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// A read-only value shown like an input (`.inp`).
pub fn field_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(bg())),
        border: Border {
            color: line2(),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// A stable colour for a name (the emitter of a message): the same agent always gets the same one.
pub fn emitter_color(name: &str) -> Color {
    let palette: [Color; 6] = [
        accent(),
        ok(),
        violet(),
        warn(),
        bad(),
        Color::from_rgb(1.0, 0.831, 0.0),
    ];
    // FNV-1a: cheap and the same on every run
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    palette[(hash % palette.len() as u64) as usize]
}

/// The colour of a message kind: needs-you and failures stand out, acknowledgements are calm.
pub fn kind_color(kind: &str, has_topic: bool) -> Color {
    match kind {
        "blocked" | "alert" | "warn" => warn(),
        "error" | "failed" | "fail" => bad(),
        "ack" | "done" | "result" => ok(),
        _ if has_topic => accent(),
        _ => dim(),
    }
}

/// A multi-line text box (`.inp`): the same dark field as a single-line input, cyan border while focused.
pub fn editor_style(
    _: &Theme,
    status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    let focused = matches!(status, iced::widget::text_editor::Status::Focused { .. });
    iced::widget::text_editor::Style {
        background: Background::Color(bg()),
        border: Border {
            color: if focused { accent() } else { line2() },
            width: 1.0,
            radius: 4.0.into(),
        },
        placeholder: dim(),
        value: fg(),
        selection: Color { a: 0.3, ..accent() },
    }
}

/// The iced theme of the app: Shepherd's palette, so that the widgets nobody styled by hand (pick lists, check
/// boxes, scrollbars, text selection) wear the same colours as the rest.
/// The fixed inspector or drawer on the right of a view: `bar` background and a hairline on its left edge.
pub fn inspector_style(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(bar())),
        border: Border {
            color: line(),
            width: 0.5,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// A thin vertical scrollbar (8 px, transparent track, thumb `line2`, lighter on hover).
pub fn scrollbar_style(
    _: &Theme,
    status: iced::widget::scrollable::Status,
) -> iced::widget::scrollable::Style {
    use iced::widget::scrollable::{Rail, Scroller, Status, Style};
    let hot = matches!(
        status,
        Status::Hovered {
            is_vertical_scrollbar_hovered: true,
            ..
        } | Status::Hovered {
            is_horizontal_scrollbar_hovered: true,
            ..
        } | Status::Dragged { .. }
    );
    let rail = Rail {
        background: None,
        border: Border::default(),
        scroller: Scroller {
            background: Background::Color(if hot { line3() } else { line2() }),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
        },
    };
    Style {
        container: iced::widget::container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: iced::widget::scrollable::AutoScroll {
            background: Background::Color(surface()),
            border: Border::default(),
            shadow: iced::Shadow::default(),
            icon: dim(),
        },
    }
}

/// A scrollable with the thin scrollbar of the design: vertical, 8 px.
pub fn scroll<'a>(
    content: impl Into<iced::Element<'a, crate::Message>>,
) -> iced::widget::Scrollable<'a, crate::Message> {
    iced::widget::scrollable(content)
        .style(scrollbar_style)
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(8)
                .scroller_width(8),
        ))
}

/// A switch (34 x 18): off a `selected` track with a faint knob, on a cyan track with a cyan knob.
pub fn switch_style(
    _: &Theme,
    status: iced::widget::toggler::Status,
) -> iced::widget::toggler::Style {
    let on = matches!(
        status,
        iced::widget::toggler::Status::Active { is_toggled: true }
            | iced::widget::toggler::Status::Hovered { is_toggled: true }
    );
    iced::widget::toggler::Style {
        background: if on {
            Background::Color(Color {
                a: 0.25,
                ..accent()
            })
        } else {
            Background::Color(selected())
        },
        background_border_width: 1.0,
        background_border_color: if on {
            Color { a: 0.6, ..accent() }
        } else {
            line2()
        },
        foreground: Background::Color(if on { accent() } else { faint() }),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: None,
        border_radius: Some(9.0.into()),
        padding_ratio: 0.2,
    }
}

pub fn app_theme() -> Theme {
    Theme::custom(
        "Prismux",
        iced::theme::Palette {
            background: bg(),
            text: fg(),
            primary: accent(),
            success: ok(),
            warning: warn(),
            danger: bad(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emitter_keeps_its_colour_and_different_ones_spread() {
        assert_eq!(emitter_color("brain-20"), emitter_color("brain-20"));
        let names = [
            "brain-20", "coder-37", "coder-38", "coder-39", "reviewer", "shepherd",
        ];
        let distinct = names
            .iter()
            .map(|n| format!("{:?}", emitter_color(n)))
            .collect::<std::collections::HashSet<_>>();
        assert!(distinct.len() >= 3, "names do not all land on one colour");
    }

    #[test]
    fn kinds_are_coloured_by_what_they_mean() {
        assert_eq!(kind_color("blocked", true), warn());
        assert_eq!(kind_color("error", false), bad());
        assert_eq!(kind_color("ack", true), ok());
        assert_eq!(kind_color("msg", true), accent());
        assert_eq!(kind_color("msg", false), dim());
    }
}
