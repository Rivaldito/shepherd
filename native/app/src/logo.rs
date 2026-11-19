//! The logo: the flock, three pixel sheep on an 18 x 9 cell grid (each sheep 8 x 5), white and greys only,
//! drawn in whole cells and never animated. The lead sheep is white; the two behind it fade into the dark.

use crate::Message;
use iced::widget::canvas::{self, Frame};
use iced::{mouse, Color, Point, Rectangle, Size, Theme};

/// One sheep: `W` wool, `H` head, `E` eye, `L` leg, `.` nothing.
const SHEEP: [&str; 5] = [".WW.WW..", "WWWWWWHE", "WWWWWWHH", "WWWWWW..", ".L..L..."];
pub const COLS: u32 = 18;
pub const ROWS: u32 = 9;

fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The sheep of the flock: `(column, row, tone)`; the last one leads.
const HERD: [(u32, u32, u8); 3] = [(0, 0, 0), (10, 0, 1), (5, 4, 2)];

/// The colour of one cell of a sheep in a tone (0 and 1 are the back sheep, 2 leads).
fn tone(tone: u8, cell: char) -> Color {
    rgb(match (tone, cell) {
        (0, 'W') => 0x4A4A5A,
        (0, 'E') => 0x07070A,
        (0, _) => 0x3A3A48,
        (1, 'W') => 0x8A8A98,
        (1, 'E') => 0x07070A,
        (1, _) => 0x6A6A78,
        (_, 'W') => 0xF4F4F8,
        (_, 'H') => 0xB4B4C0,
        (_, 'E') => 0x07070A,
        _ => 0x9A9AA8,
    })
}

/// The cells of the flock: `(column, row, colour)`.
pub fn cells() -> Vec<(u32, u32, Color)> {
    let mut out = Vec::new();
    for (x0, y0, t) in HERD {
        for (ry, row) in SHEEP.iter().enumerate() {
            for (rx, ch) in row.chars().enumerate() {
                if ch != '.' {
                    out.push((x0 + rx as u32, y0 + ry as u32, tone(t, ch)));
                }
            }
        }
    }
    out
}

pub struct Flock;

impl canvas::Program<Message> for Flock {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        // whole cells only: the biggest square cell that fits, centred
        let u = (bounds.width / COLS as f32)
            .min(bounds.height / ROWS as f32)
            .floor()
            .max(1.0);
        let ox = ((bounds.width - u * COLS as f32) / 2.0).floor();
        let oy = ((bounds.height - u * ROWS as f32) / 2.0).floor();
        for (x, y, color) in cells() {
            frame.fill_rectangle(
                Point::new(ox + x as f32 * u, oy + y as f32 * u),
                Size::new(u, u),
                color,
            );
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flock_fits_its_grid_and_the_lead_sheep_is_in_front() {
        let cells = cells();
        assert!(cells.iter().all(|(x, y, _)| *x < COLS && *y < ROWS));
        // a sheep has 4 + 8 + 8 + 6 + 2 = 28 cells
        assert_eq!(cells.len(), 3 * 28);
        // the lead sheep's wool is the brightest cell of the logo
        let lead_wool = cells.iter().find(|(x, y, _)| (*x, *y) == (6, 5)).unwrap().2;
        assert_eq!(lead_wool, rgb(0xF4F4F8));
    }
}
