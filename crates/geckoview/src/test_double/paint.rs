//! A page-shaped picture for the test double: a header, a hero, columns of
//! "text" bars and a link, drawn into BGRX memory. It is wireframe, not
//! typography: the browser chrome around it is what is under test, and a real
//! page appears when the real engine is behind the same trait.

/// The area of the page that is a link: where the double reports a hover
/// and where a right click finds a link, in the page's own pixels (before
/// scroll).
pub const LINK: (f32, f32, f32, f32) = (40.0, 300.0, 220.0, 24.0);

/// The address the link goes to.
pub const LINK_URL: &str = "https://example.com/more-information";

/// One tall page's height in CSS pixels, for scrolling.
pub const PAGE_HEIGHT: f32 = 1800.0;

fn hsl(hue: f32, sat: f32, light: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * light - 1.0).abs()) * sat;
    let h = (hue.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = light - c / 2.0;
    [r + m, g + m, b + m].map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// A canvas of device pixels that is drawn in logical ones.
struct Canvas {
    width: usize,
    height: usize,
    scale: f32,
    bytes: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32, scale: f32) -> Self {
        Self {
            width: width as usize,
            height: height as usize,
            scale,
            bytes: vec![0; width as usize * height as usize * 4],
        }
    }

    /// Fill a rectangle given in logical pixels, `scroll` already applied.
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, colour: [u8; 3]) {
        let to_device = |value: f32| (value * self.scale).round() as i64;
        let (x0, y0) = (to_device(x).max(0), to_device(y).max(0));
        let (x1, y1) = (
            to_device(x + w).min(self.width as i64),
            to_device(y + h).min(self.height as i64),
        );
        for row in y0..y1 {
            let start = (row as usize * self.width + x0 as usize) * 4;
            for pixel in 0..(x1 - x0).max(0) as usize {
                let at = start + pixel * 4;
                self.bytes[at] = colour[2];
                self.bytes[at + 1] = colour[1];
                self.bytes[at + 2] = colour[0];
                self.bytes[at + 3] = 255;
            }
        }
    }
}

/// What kind of page to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A blank document.
    Blank,
    /// A site: header, hero, text columns, a link.
    Site,
    /// Video: a large dark player and a row of thumbnails.
    Video,
}

/// Draws the page at `scroll` logical pixels down, `width` x `height` device
/// pixels at `scale`. `hue` gives each site its own colour.
pub fn page(look: Look, hue: f32, scroll: f32, width: u32, height: u32, scale: f32) -> Vec<u8> {
    let mut canvas = Canvas::new(width, height, scale);
    let logical_width = width as f32 / scale;
    canvas.fill(0.0, 0.0, logical_width, height as f32 / scale, [255, 255, 255]);
    if look == Look::Blank {
        return canvas.bytes;
    }
    let accent = hsl(hue, 0.65, 0.45);
    let soft = hsl(hue, 0.55, 0.93);
    let ink = [38, 41, 48];
    let faint = [208, 211, 218];
    let at = |y: f32| y - scroll;
    // Header bar and its links.
    canvas.fill(0.0, at(0.0), logical_width, 56.0, accent);
    canvas.fill(24.0, at(18.0), 96.0, 20.0, [255, 255, 255]);
    for index in 0..4 {
        canvas.fill(logical_width - 360.0 + index as f32 * 84.0, at(22.0), 64.0, 12.0, soft);
    }
    match look {
        Look::Video => {
            canvas.fill(24.0, at(80.0), logical_width - 48.0, 360.0, [20, 22, 26]);
            canvas.fill(logical_width / 2.0 - 24.0, at(240.0), 48.0, 48.0, [240, 240, 240]);
            canvas.fill(24.0, at(420.0), logical_width - 48.0, 6.0, [90, 92, 98]);
            canvas.fill(24.0, at(420.0), (logical_width - 48.0) * 0.37, 6.0, accent);
        }
        _ => {
            // Hero.
            canvas.fill(0.0, at(56.0), logical_width, 190.0, soft);
            canvas.fill(40.0, at(92.0), 360.0, 28.0, ink);
            canvas.fill(40.0, at(136.0), 280.0, 12.0, [110, 114, 124]);
            canvas.fill(40.0, at(158.0), 320.0, 12.0, [110, 114, 124]);
            canvas.fill(40.0, at(190.0), 120.0, 36.0, accent);
            // The link.
            canvas.fill(LINK.0, at(LINK.1), LINK.2, 3.0, accent);
            canvas.fill(LINK.0, at(LINK.1 - 18.0), LINK.2 - 40.0, 12.0, accent);
            // Columns of text.
            let column = ((logical_width - 48.0) / 3.0).max(80.0);
            for col in 0..3 {
                let x = 24.0 + col as f32 * (column + 0.0);
                canvas.fill(x, at(350.0), column - 16.0, 110.0, faint);
                canvas.fill(x, at(480.0), column * 0.6, 16.0, ink);
                for line in 0..9 {
                    let w = column - 16.0 - ((line * 37 + col * 23) % 60) as f32;
                    canvas.fill(x, at(512.0 + line as f32 * 20.0), w, 8.0, [140, 144, 154]);
                }
            }
            // More below the fold, so the page scrolls.
            for block in 0..6 {
                canvas.fill(24.0, at(760.0 + block as f32 * 170.0), logical_width - 48.0, 120.0, if block % 2 == 0 { soft } else { faint });
            }
        }
    }
    canvas.bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
        let at = (y as usize * width as usize + x as usize) * 4;
        [bytes[at + 2], bytes[at + 1], bytes[at]]
    }

    #[test]
    fn a_blank_page_is_white_and_opaque() {
        let bytes = page(Look::Blank, 0.0, 0.0, 8, 8, 1.0);
        assert_eq!(bytes.len(), 8 * 8 * 4);
        assert!(bytes.chunks(4).all(|px| px == [255, 255, 255, 255]));
    }

    #[test]
    fn scrolling_moves_the_content_up() {
        let top = page(Look::Site, 210.0, 0.0, 400, 300, 1.0);
        let scrolled = page(Look::Site, 210.0, 56.0, 400, 300, 1.0);
        let header = pixel(&top, 400, 10, 10);
        assert_ne!(header, [255, 255, 255], "the header is coloured at the top");
        assert_eq!(pixel(&scrolled, 400, 10, 10), pixel(&top, 400, 10, 66), "what was 56 px down is at the top");
    }

    #[test]
    fn a_scaled_frame_draws_the_same_picture_in_device_pixels() {
        let one = page(Look::Site, 90.0, 0.0, 400, 300, 1.0);
        let two = page(Look::Site, 90.0, 0.0, 800, 600, 2.0);
        assert_eq!(pixel(&one, 400, 10, 10), pixel(&two, 800, 20, 20));
    }
}
