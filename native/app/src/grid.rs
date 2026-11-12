//! The terminal area: paints the server's cell grid and turns mouse input into messages.

use crate::palette;
use iced::mouse;
use iced::widget::canvas;
use iced::{Font, Point, Rectangle, Size, Theme};
use shepherd_app_core::wire::{style, Color as Cell, PaneSurface, PaneSurfacePane};

/// JetBrains Mono at 13 px: 0.6 em of advance, and 17 px of line.
pub const CELL_W: f32 = 7.8;
pub const CELL_H: f32 = 17.0;
pub const FONT_SIZE: f32 = 13.0;

/// What happened in the terminal area, in terminal cells.
#[derive(Debug, Clone)]
pub enum Event {
    Press {
        pane: String,
        focused: bool,
        mouse_reporting: bool,
        column: u16,
        row: u16,
    },
    Release {
        pane: String,
        mouse_reporting: bool,
        column: u16,
        row: u16,
    },
    Scroll {
        pane: String,
        up: bool,
        column: u16,
        row: u16,
    },
}

pub struct Grid<'a> {
    pub surface: &'a PaneSurface,
}

/// The pane under a cell of the surface and the cell relative to its content.
pub fn pane_at(
    surface: &PaneSurface,
    column: u16,
    row: u16,
) -> Option<(&PaneSurfacePane, u16, u16)> {
    surface.panes.iter().find_map(|pane| {
        let r = pane.rect;
        let inside = column >= r.x && column < r.x + r.width && row >= r.y && row < r.y + r.height;
        inside.then(|| {
            let inner = pane.inner_rect;
            (
                pane,
                column.saturating_sub(inner.x),
                row.saturating_sub(inner.y),
            )
        })
    })
}

/// A rectangle inside one cell, in pixels from the cell's corner, and how opaque to paint it.
type Piece = (f32, f32, f32, f32, f32);

/// Block elements and box-drawing characters as rectangles that fill the cell exactly. A font never does:
/// its glyphs leave gaps between rows and between neighbours, which breaks the borders, tables and block
/// logos that terminal programs draw with them. `None` for anything else (it is drawn as text).
fn pieces(ch: char) -> Option<Vec<Piece>> {
    let (w, h) = (CELL_W, CELL_H);
    let full = |x: f32, y: f32, pw: f32, ph: f32| (x, y, pw, ph, 1.0);
    let c = ch as u32;
    // U+2580..U+259F: halves, eighths, shades and quadrants
    if (0x2580..=0x259F).contains(&c) {
        let quadrants = |ul: bool, ur: bool, ll: bool, lr: bool| {
            let mut v = Vec::new();
            for (on, x, y) in [
                (ul, 0.0, 0.0),
                (ur, w / 2.0, 0.0),
                (ll, 0.0, h / 2.0),
                (lr, w / 2.0, h / 2.0),
            ] {
                if on {
                    v.push(full(x, y, w / 2.0, h / 2.0));
                }
            }
            v
        };
        return Some(match c {
            0x2580 => vec![full(0.0, 0.0, w, h / 2.0)],
            0x2581..=0x2588 => {
                let ph = h * (c - 0x2580) as f32 / 8.0;
                vec![full(0.0, h - ph, w, ph)]
            }
            0x2589..=0x258F => vec![full(0.0, 0.0, w * (0x2590 - c) as f32 / 8.0, h)],
            0x2590 => vec![full(w / 2.0, 0.0, w / 2.0, h)],
            0x2591 => vec![(0.0, 0.0, w, h, 0.25)],
            0x2592 => vec![(0.0, 0.0, w, h, 0.5)],
            0x2593 => vec![(0.0, 0.0, w, h, 0.75)],
            0x2594 => vec![full(0.0, 0.0, w, h / 8.0)],
            0x2595 => vec![full(w - w / 8.0, 0.0, w / 8.0, h)],
            0x2596 => quadrants(false, false, true, false),
            0x2597 => quadrants(false, false, false, true),
            0x2598 => quadrants(true, false, false, false),
            0x2599 => quadrants(true, false, true, true),
            0x259A => quadrants(true, false, false, true),
            0x259B => quadrants(true, true, true, false),
            0x259C => quadrants(true, true, false, true),
            0x259D => quadrants(false, true, false, false),
            0x259E => quadrants(false, true, true, false),
            _ => quadrants(false, true, true, true), // 0x259F
        });
    }
    // box drawing: (left, right, up, down) with 0 none, 1 light, 2 heavy (double lines are drawn heavy)
    let (l, r, u, d): (u8, u8, u8, u8) = match ch {
        '─' => (1, 1, 0, 0),
        '━' | '═' => (2, 2, 0, 0),
        '│' => (0, 0, 1, 1),
        '┃' | '║' => (0, 0, 2, 2),
        '┌' | '╭' => (0, 1, 0, 1),
        '┏' | '╔' => (0, 2, 0, 2),
        '┐' | '╮' => (1, 0, 0, 1),
        '┓' | '╗' => (2, 0, 0, 2),
        '└' | '╰' => (0, 1, 1, 0),
        '┗' | '╚' => (0, 2, 2, 0),
        '┘' | '╯' => (1, 0, 1, 0),
        '┛' | '╝' => (2, 0, 2, 0),
        '├' => (0, 1, 1, 1),
        '┣' | '╠' => (0, 2, 2, 2),
        '┤' => (1, 0, 1, 1),
        '┫' | '╣' => (2, 0, 2, 2),
        '┬' => (1, 1, 0, 1),
        '┳' | '╦' => (2, 2, 0, 2),
        '┴' => (1, 1, 1, 0),
        '┻' | '╩' => (2, 2, 2, 0),
        '┼' => (1, 1, 1, 1),
        '╋' | '╬' => (2, 2, 2, 2),
        '╴' => (1, 0, 0, 0),
        '╵' => (0, 0, 1, 0),
        '╶' => (0, 1, 0, 0),
        '╷' => (0, 0, 0, 1),
        _ => return None,
    };
    let (cx, cy) = (w / 2.0, h / 2.0);
    let t = |weight: u8| if weight == 2 { 2.0 } else { 1.0 };
    let reach = t(l.max(r).max(u).max(d)) / 2.0;
    let mut v = Vec::new();
    // each arm runs from the cell's edge to the middle, so neighbouring cells meet without a gap
    if l > 0 {
        v.push(full(0.0, cy - t(l) / 2.0, cx + reach, t(l)));
    }
    if r > 0 {
        v.push(full(cx - reach, cy - t(r) / 2.0, w - cx + reach, t(r)));
    }
    if u > 0 {
        v.push(full(cx - t(u) / 2.0, 0.0, t(u), cy + reach));
    }
    if d > 0 {
        v.push(full(cx - t(d) / 2.0, cy - reach, t(d), h - cy + reach));
    }
    Some(v)
}

fn cell_of(bounds: Rectangle, cursor: mouse::Cursor) -> Option<(u16, u16)> {
    let position = cursor.position_in(bounds)?;
    Some(((position.x / CELL_W) as u16, (position.y / CELL_H) as u16))
}

impl canvas::Program<crate::Message> for Grid<'_> {
    type State = GridState;

    fn update(
        &self,
        _state: &mut GridState,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<crate::Message>> {
        let iced::Event::Mouse(event) = event else {
            return None;
        };
        let (column, row) = cell_of(bounds, cursor)?;
        let (pane, rel_column, rel_row) = pane_at(self.surface, column, row)?;
        let id = pane.pane_id.clone();
        let event = match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => Event::Press {
                pane: id,
                focused: pane.focused,
                mouse_reporting: pane.mouse_reporting,
                column: rel_column,
                row: rel_row,
            },
            mouse::Event::ButtonReleased(mouse::Button::Left) => Event::Release {
                pane: id,
                mouse_reporting: pane.mouse_reporting,
                column: rel_column,
                row: rel_row,
            },
            mouse::Event::WheelScrolled { delta } => {
                let y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y,
                };
                if y == 0.0 {
                    return None;
                }
                Event::Scroll {
                    pane: id,
                    up: y > 0.0,
                    column: rel_column,
                    row: rel_row,
                }
            }
            _ => return None,
        };
        Some(canvas::Action::publish(crate::Message::Grid(event)).and_capture())
    }

    fn draw(
        &self,
        state: &GridState,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        // the cells only change with the surface; the cursor is painted apart so it never forces a repaint
        let key = (
            self.surface.projection_revision,
            self.surface.surface_revision,
            self.surface.frame.width,
            self.surface.frame.height,
        );
        if state.key.get() != Some(key) {
            state.cache.clear();
            state.key.set(Some(key));
        }
        let cells = state.cache.draw(renderer, bounds.size(), |frame| {
            self.paint_cells(frame, bounds)
        });
        let mut out = vec![cells];
        if let Some(cursor) = self.surface.frame.cursor.as_ref().filter(|c| c.visible) {
            let mut frame = canvas::Frame::new(renderer, bounds.size());
            frame.fill_rectangle(
                Point::new(cursor.x as f32 * CELL_W, cursor.y as f32 * CELL_H),
                Size::new(CELL_W, CELL_H),
                iced::Color {
                    a: 0.45,
                    ..palette::DEFAULT_FG
                },
            );
            out.push(frame.into_geometry());
        }
        out
    }
}

/// What the grid keeps between frames: the painted cells and the surface they were painted from.
#[derive(Default)]
pub struct GridState {
    cache: canvas::Cache,
    key: std::cell::Cell<Option<(u64, u64, u16, u16)>>,
}

/// How a run of text looks: all of its cells share it.
#[derive(Clone, Copy, PartialEq)]
struct Look {
    color: iced::Color,
    bold: bool,
    italic: bool,
}

impl Grid<'_> {
    fn paint_cells(&self, frame: &mut canvas::Frame, bounds: Rectangle) {
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), palette::DEFAULT_BG);
        let data = &self.surface.frame;
        let width = data.width.max(1) as usize;
        let origin_of = |index: usize| {
            Point::new(
                (index % width) as f32 * CELL_W,
                (index / width) as f32 * CELL_H,
            )
        };
        // what each cell looks like once reverse video and dim are applied
        let resolve = |cell: &shepherd_app_core::wire::CellData| {
            let (mut fg, mut bg) = (
                palette::resolve(Cell::unpack(cell.fg)),
                palette::resolve(Cell::unpack(cell.bg)),
            );
            if cell.modifier & style::REVERSED != 0 {
                (fg, bg) = (
                    Some(bg.unwrap_or(palette::DEFAULT_BG)),
                    Some(fg.unwrap_or(palette::DEFAULT_FG)),
                );
            }
            let mut color = fg.unwrap_or(palette::DEFAULT_FG);
            if cell.modifier & style::DIM != 0 {
                color.a = 0.6;
            }
            (color, bg)
        };

        // backgrounds first, a neighbouring run of one colour as a single rectangle
        let mut run: Option<(usize, usize, iced::Color)> = None; // (first index, cells, colour)
        let flush = |frame: &mut canvas::Frame, run: &mut Option<(usize, usize, iced::Color)>| {
            if let Some((first, n, color)) = run.take() {
                frame.fill_rectangle(
                    origin_of(first),
                    Size::new(CELL_W * n as f32, CELL_H),
                    color,
                );
            }
        };
        for (index, cell) in data.cells.iter().enumerate() {
            let bg = if cell.skip { None } else { resolve(cell).1 };
            match (bg, run.as_mut()) {
                (Some(color), Some((first, n, c)))
                    if *c == color && (*first + *n) % width != 0 && *first + *n == index =>
                {
                    *n += 1;
                }
                (Some(color), _) => {
                    flush(frame, &mut run);
                    run = Some((index, 1, color));
                }
                (None, _) => flush(frame, &mut run),
            }
        }
        flush(frame, &mut run);

        // then the text: plain ASCII of one look on one row is drawn as one string, the rest cell by cell
        let mut text: Option<(usize, String, Look)> = None;
        let put = |frame: &mut canvas::Frame, text: &mut Option<(usize, String, Look)>| {
            if let Some((first, content, look)) = text.take() {
                frame.fill_text(canvas::Text {
                    content,
                    position: origin_of(first),
                    color: look.color,
                    size: FONT_SIZE.into(),
                    font: font_of(look),
                    ..canvas::Text::default()
                });
            }
        };
        for (index, cell) in data.cells.iter().enumerate() {
            if cell.skip || cell.symbol.trim().is_empty() {
                // a gap ends the run, the next word starts its own
                put(frame, &mut text);
                continue;
            }
            let (color, _) = resolve(cell);
            let origin = origin_of(index);
            let mut chars = cell.symbol.chars();
            let single = match (chars.next(), chars.next()) {
                (Some(ch), None) => Some(ch),
                _ => None,
            };
            if let Some(ch) = single {
                // a record dot is a plain circle: the font gives a wide colour emoji that covers its neighbour
                if ch == '\u{23FA}' {
                    put(frame, &mut text);
                    let circle = canvas::Path::circle(
                        Point::new(origin.x + CELL_W / 2.0, origin.y + CELL_H / 2.0),
                        CELL_W * 0.4,
                    );
                    frame.fill(&circle, color);
                    continue;
                }
                if let Some(parts) = pieces(ch) {
                    put(frame, &mut text);
                    for (x, y, w, h, alpha) in parts {
                        frame.fill_rectangle(
                            Point::new(origin.x + x, origin.y + y),
                            Size::new(w, h),
                            iced::Color {
                                a: color.a * alpha,
                                ..color
                            },
                        );
                    }
                    continue;
                }
            }
            let look = Look {
                color,
                bold: cell.modifier & style::BOLD != 0,
                italic: cell.modifier & style::ITALIC != 0,
            };
            let plain = single.is_some_and(|c| c.is_ascii_graphic());
            match text.as_mut() {
                Some((first, content, l))
                    if plain
                        && *l == look
                        && *first + content.len() == index
                        && index % width != 0 =>
                {
                    content.push_str(&cell.symbol);
                }
                _ => {
                    put(frame, &mut text);
                    if plain {
                        text = Some((index, cell.symbol.clone(), look));
                    } else {
                        // a wide or fallback glyph keeps its own cell so it cannot shift its neighbours
                        frame.fill_text(canvas::Text {
                            content: cell.symbol.clone(),
                            position: origin,
                            color,
                            size: FONT_SIZE.into(),
                            font: font_of(look),
                            ..canvas::Text::default()
                        });
                    }
                }
            }
            if cell.modifier & style::UNDERLINED != 0 {
                frame.fill_rectangle(
                    Point::new(origin.x, origin.y + CELL_H - 2.0),
                    Size::new(CELL_W, 1.0),
                    color,
                );
            }
            if cell.modifier & style::CROSSED_OUT != 0 {
                frame.fill_rectangle(
                    Point::new(origin.x, origin.y + CELL_H / 2.0),
                    Size::new(CELL_W, 1.0),
                    color,
                );
            }
        }
        put(frame, &mut text);
    }
}

fn font_of(look: Look) -> Font {
    Font {
        weight: if look.bold {
            iced::font::Weight::Bold
        } else {
            iced::font::Weight::Normal
        },
        style: if look.italic {
            iced::font::Style::Italic
        } else {
            iced::font::Style::Normal
        },
        ..crate::theme::MONO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepherd_app_core::wire::{FrameData, SurfaceRect};

    fn pane(id: &str, x: u16, w: u16) -> PaneSurfacePane {
        PaneSurfacePane {
            pane_id: id.into(),
            content_revision: 0,
            rect: SurfaceRect {
                x,
                y: 0,
                width: w,
                height: 10,
            },
            inner_rect: SurfaceRect {
                x: x + 1,
                y: 1,
                width: w - 2,
                height: 8,
            },
            scrollbar_rect: None,
            scroll: None,
            focused: false,
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 0,
            pixel_height: 0,
        }
    }

    #[test]
    fn blocks_and_box_lines_fill_their_cell_without_gaps() {
        // a full block is the whole cell, a half block is half of it
        assert_eq!(pieces('█'), Some(vec![(0.0, 0.0, CELL_W, CELL_H, 1.0)]));
        assert_eq!(
            pieces('▀'),
            Some(vec![(0.0, 0.0, CELL_W, CELL_H / 2.0, 1.0)])
        );
        // the logo's quadrant blocks: ▛ is three quarters, ▝ one
        assert_eq!(pieces('▛').map(|p| p.len()), Some(3));
        assert_eq!(pieces('▝').map(|p| p.len()), Some(1));
        // a shade is the whole cell, translucent
        assert_eq!(pieces('▒').map(|p| p[0].4), Some(0.5));
        // a vertical line reaches both edges of its cell, so rows join up
        let v = pieces('│').unwrap();
        assert_eq!((v[0].1, v[1].1 + v[1].3), (0.0, CELL_H));
        // a corner has two arms, a cross four; rounded corners are drawn like square ones
        assert_eq!(pieces('┌').map(|p| p.len()), Some(2));
        assert_eq!(pieces('┼').map(|p| p.len()), Some(4));
        assert_eq!(pieces('╭'), pieces('┌'));
        // ordinary text is not drawn here
        assert_eq!(pieces('a'), None);
        assert_eq!(pieces('日'), None);
    }

    #[test]
    fn a_click_finds_its_pane_and_the_cell_inside_it() {
        let surface = PaneSurface {
            boot_id: "b".into(),
            projection_revision: 1,
            surface_revision: 1,
            frame: FrameData {
                cells: vec![],
                width: 40,
                height: 10,
                cursor: None,
                hyperlinks: vec![],
                graphics: vec![],
            },
            panes: vec![pane("left", 0, 20), pane("right", 20, 20)],
            splits: vec![],
        };
        let (p, c, r) = pane_at(&surface, 25, 4).unwrap();
        assert_eq!((p.pane_id.as_str(), c, r), ("right", 4, 3));
        assert_eq!(
            pane_at(&surface, 3, 0).map(|(p, c, r)| (p.pane_id.as_str(), c, r)),
            Some(("left", 2, 0)),
            "the border row clamps to the content"
        );
        assert!(pane_at(&surface, 45, 4).is_none());
    }
}
