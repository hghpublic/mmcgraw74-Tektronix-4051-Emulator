//! Maps the 4051 display PIA signals to a persistent green-channel canvas.
//!
//! Buffer-clock events move the beam and may draw vectors; character plotting
//! combines start-of-text and alpha-dot requests. Stored and cursor pixels use
//! different intensities so a host refresh can remove transient marks.

const SCREEN_X_OFFSET: i32 = 16;
const SCREEN_Y_OFFSET: i32 = 10;
const CHARACTER_DOT_SCALE: i32 = 2;
const FINE_DOT_SIZE: usize = 1;
const COARSE_DOT_SIZE: usize = 2;
const FINE_PIXEL_STORE_INTENSITY: u8 = 254;
const FINE_PIXEL_CURSOR_INTENSITY: u8 = 239;
const COARSE_PIXEL_STORE_INTENSITY: u8 = 240;
const COARSE_PIXEL_CURSOR_INTENSITY: u8 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayGranularity {
    Fine,
    Coarse,
    Mixed,
}

#[derive(Clone, Copy)]
pub(crate) struct DisplayControl {
    pub(crate) x_character: u8,
    pub(crate) y_character: u8,
    pub(crate) vector_0: bool,
    pub(crate) ven_1: bool,
    pub(crate) x: u16,
    pub(crate) y: u16,
}

#[derive(Clone, Copy)]
pub(crate) enum ScreenCommand {
    BufferClock,
    StartOfText,
    AlphaDot,
}

#[derive(Clone, Copy)]
enum PixelKind {
    Cursor,
    Stored,
}

/// Stores the reference canvas as row-major green-channel intensity values.
pub struct DisplayModel {
    pixels: Vec<u8>,
    revision: u64,
    x_da: i32,
    y_da: i32,
    adot_pending: bool,
    granularity: DisplayGranularity,
    px_dot_size: usize,
    adot_size: usize,
    pixel_store_intensity: u8,
    pixel_cursor_intensity: u8,
}

impl DisplayModel {
    pub const WIDTH: usize = 1046;
    pub const HEIGHT: usize = 830;

    /// Returns the canvas width in pixels.
    pub const fn width(&self) -> usize {
        Self::WIDTH
    }

    /// Returns the canvas height in pixels.
    pub const fn height(&self) -> usize {
        Self::HEIGHT
    }

    /// Returns immutable green-channel intensities in canvas row-major order.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns a wrapping counter that changes when display output is updated.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns a canvas pixel's green-channel intensity, or `None` outside the canvas.
    pub fn pixel_intensity(&self, x: usize, y: usize) -> Option<u8> {
        if x >= Self::WIDTH || y >= Self::HEIGHT {
            return None;
        }
        Some(self.pixels[y * Self::WIDTH + x])
    }

    /// Returns the active fine, coarse, or mixed pixel-size mode.
    pub const fn granularity(&self) -> DisplayGranularity {
        self.granularity
    }

    /// Selects the source's dot sizes and brightness thresholds.
    pub fn set_granularity(&mut self, granularity: DisplayGranularity) {
        let (px_dot_size, adot_size, pixel_store_intensity, pixel_cursor_intensity) =
            match granularity {
                DisplayGranularity::Fine => (
                    FINE_DOT_SIZE,
                    FINE_DOT_SIZE,
                    FINE_PIXEL_STORE_INTENSITY,
                    FINE_PIXEL_CURSOR_INTENSITY,
                ),
                DisplayGranularity::Coarse => (
                    COARSE_DOT_SIZE,
                    COARSE_DOT_SIZE,
                    COARSE_PIXEL_STORE_INTENSITY,
                    COARSE_PIXEL_CURSOR_INTENSITY,
                ),
                DisplayGranularity::Mixed => (
                    FINE_DOT_SIZE,
                    COARSE_DOT_SIZE,
                    FINE_PIXEL_STORE_INTENSITY,
                    FINE_PIXEL_CURSOR_INTENSITY,
                ),
            };
        self.granularity = granularity;
        self.px_dot_size = px_dot_size;
        self.adot_size = adot_size;
        self.pixel_store_intensity = pixel_store_intensity;
        self.pixel_cursor_intensity = pixel_cursor_intensity;
    }

    pub(crate) fn apply_screen_command(&mut self, command: ScreenCommand, control: DisplayControl) {
        let new_x = i32::from(control.x) + SCREEN_X_OFFSET;
        let new_y = i32::from(control.y) + SCREEN_Y_OFFSET;

        match command {
            ScreenCommand::BufferClock => {
                if !control.vector_0 {
                    self.draw_vector(new_x, new_y, control.ven_1);
                }
                self.x_da = new_x;
                self.y_da = new_y;
            }
            ScreenCommand::StartOfText if control.vector_0 && !control.ven_1 => {
                let x = self.x_da + i32::from(control.x_character) * CHARACTER_DOT_SCALE;
                let y = self.y_da + i32::from(control.y_character) * CHARACTER_DOT_SCALE;
                if self.adot_pending {
                    let size = if self.x_da == new_x && self.y_da == new_y {
                        self.px_dot_size
                    } else {
                        self.adot_size
                    };
                    self.set_pixel(x, y, PixelKind::Stored, size);
                } else {
                    self.set_pixel(x, y, PixelKind::Cursor, 1);
                    self.set_pixel(x + 1, y + 1, PixelKind::Cursor, 1);
                }
                self.adot_pending = false;
            }
            ScreenCommand::AlphaDot if control.vector_0 && !control.ven_1 => {
                self.adot_pending = true;
            }
            ScreenCommand::StartOfText | ScreenCommand::AlphaDot => {}
        }
        self.revision = self.revision.wrapping_add(1);
    }

    // The reference phosphor callback removes cursor-level pixels and retains stored vectors/dots.
    pub(crate) fn refresh(&mut self) {
        let mut changed = false;
        for pixel in &mut self.pixels {
            if self.pixel_store_intensity > *pixel {
                *pixel = 0;
                changed = true;
            }
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    // ERASE presents a full-bright green canvas until the host's 500 ms completion callback.
    pub(crate) fn begin_erase(&mut self) {
        self.pixels.fill(u8::MAX);
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn complete_erase(&mut self) {
        self.pixels.fill(0);
        self.revision = self.revision.wrapping_add(1);
    }

    fn draw_vector(&mut self, x1: i32, y1: i32, ven_1: bool) {
        let mut x = self.x_da;
        let mut y = self.y_da;
        let dx = (x1 - x).abs();
        let dy = (y1 - y).abs();
        let step_x = if x < x1 { 1 } else { -1 };
        let step_y = if y < y1 { 1 } else { -1 };
        let mut error = dx - dy;

        loop {
            if ven_1 {
                self.set_pixel(x, y, PixelKind::Stored, self.px_dot_size);
            } else {
                self.set_pixel(x + 1, y + 1, PixelKind::Cursor, 1);
            }
            if x == x1 && y == y1 {
                break;
            }

            let doubled_error = 2 * error;
            if doubled_error > -dy {
                error -= dy;
                x += step_x;
            }
            if doubled_error < dx {
                error += dx;
                y += step_y;
            }
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, kind: PixelKind, size: usize) {
        // Convert the Tek y-up coordinate to the canvas y-down row, clipping fillRect at edges.
        let canvas_y = Self::HEIGHT as i32 - (y + 1);
        for y_offset in 0..size {
            let row = canvas_y + y_offset as i32;
            if !(0..Self::HEIGHT as i32).contains(&row) {
                continue;
            }
            for x_offset in 0..size {
                let column = x + x_offset as i32;
                if !(0..Self::WIDTH as i32).contains(&column) {
                    continue;
                }
                let index = row as usize * Self::WIDTH + column as usize;
                match kind {
                    PixelKind::Cursor if self.pixels[index] != self.pixel_store_intensity => {
                        self.pixels[index] = self.pixel_cursor_intensity;
                    }
                    PixelKind::Cursor => {}
                    PixelKind::Stored => self.pixels[index] = self.pixel_store_intensity,
                }
            }
        }
    }
}

impl Default for DisplayModel {
    fn default() -> Self {
        Self {
            pixels: vec![0; Self::WIDTH * Self::HEIGHT],
            revision: 0,
            x_da: 0,
            y_da: 0,
            adot_pending: false,
            granularity: DisplayGranularity::Coarse,
            px_dot_size: COARSE_DOT_SIZE,
            adot_size: COARSE_DOT_SIZE,
            pixel_store_intensity: COARSE_PIXEL_STORE_INTENSITY,
            pixel_cursor_intensity: COARSE_PIXEL_CURSOR_INTENSITY,
        }
    }
}
