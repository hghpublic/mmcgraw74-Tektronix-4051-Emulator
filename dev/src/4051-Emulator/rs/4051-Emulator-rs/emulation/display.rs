//! CPU reference renderer for the 4051 emulator display.
//!
//! The emulation layer consumes ordered machine display operations, maintains the
//! reference raster, and publishes semantic events for readiness and glow. Vector
//! strokes remain in the base image; refresh-only cursor marks do not. Erase start
//! clears prior geometry, while strokes drawn during its delayed flash survive completion.

use std::collections::{HashSet, VecDeque};

use crate::machine::display::{DisplayControl, DisplayOperation, ScreenCommand};

const SCREEN_X_OFFSET: i32 = 16;
const SCREEN_Y_OFFSET: i32 = 10;
const CHARACTER_DOT_SCALE: i32 = 2;
const FINE_DOT_SIZE: usize = 1;
const COARSE_DOT_SIZE: usize = 2;
const FINE_PIXEL_STORE_INTENSITY: u8 = 254;
const FINE_PIXEL_CURSOR_INTENSITY: u8 = 239;
const COARSE_PIXEL_STORE_INTENSITY: u8 = 240;
const COARSE_PIXEL_CURSOR_INTENSITY: u8 = 200;
pub(crate) const MAX_PENDING_DISPLAY_EVENTS: usize = 1024;
const MAX_PENDING_SCREEN_COPIES: usize = 8;

/// Selects the reference renderer's dot sizes and brightness thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayGranularity {
    Fine,
    Coarse,
    Mixed,
}

#[derive(Clone, Copy)]
enum PixelKind {
    Cursor,
    Stored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DisplayPoint {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayEvent {
    TransientCursorMark(DisplayPoint),
    StoredMark {
        position: DisplayPoint,
        size: usize,
    },
    /// Vector geometry with an independently refreshable glow contribution.
    TransientVector {
        from: DisplayPoint,
        to: DisplayPoint,
    },
    StoredVector {
        from: DisplayPoint,
        to: DisplayPoint,
        size: usize,
    },
    Erase,
}

pub(crate) struct DisplayEventBatch {
    pub(crate) events: Vec<DisplayEvent>,
    pub(crate) dropped: bool,
}

/// Renders machine display operations into the CPU reference pixel frame.
///
/// The renderer retains vector geometry and stored marks independently of
/// egui, Canvas, or GPU presentation resources; cursor marks still refresh away.
pub struct CpuReferenceRenderer {
    pixels: Vec<u8>,
    // Holds stored pixels drawn after erase starts while the flash covers the frame.
    erase_retained_pixels: Vec<u8>,
    revision: u64,
    x_da: i32,
    y_da: i32,
    adot_pending: bool,
    granularity: DisplayGranularity,
    px_dot_size: usize,
    adot_size: usize,
    pixel_store_intensity: u8,
    pixel_cursor_intensity: u8,
    events: VecDeque<DisplayEvent>,
    queued_cursor_marks: HashSet<DisplayPoint>,
    events_dropped: bool,
    screen_copies: Vec<Vec<u8>>,
    dropped_screen_copies: usize,
}

impl CpuReferenceRenderer {
    /// Reference-frame width in pixels.
    pub const WIDTH: usize = 1046;
    /// Reference-frame height in pixels.
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

    /// Applies ordered machine operations, preserving display-copy event timing.
    pub fn apply_operations(&mut self, operations: impl IntoIterator<Item = DisplayOperation>) {
        for operation in operations {
            match operation {
                DisplayOperation::ScreenCommand { command, control } => {
                    self.apply_screen_command(command, control);
                }
                DisplayOperation::BeginErase => self.begin_erase(),
                DisplayOperation::CompleteErase => self.complete_erase(),
                DisplayOperation::ScreenCopy => {
                    if self.screen_copies.len() < MAX_PENDING_SCREEN_COPIES {
                        self.screen_copies.push(self.pixels.clone());
                    } else {
                        self.dropped_screen_copies = self.dropped_screen_copies.saturating_add(1);
                    }
                }
            }
        }
    }

    fn apply_screen_command(&mut self, command: ScreenCommand, control: DisplayControl) {
        let new_x = i32::from(control.x) + SCREEN_X_OFFSET;
        let new_y = i32::from(control.y) + SCREEN_Y_OFFSET;
        let mut pixels_changed = false;

        match command {
            ScreenCommand::BufferClock => {
                if !control.vector_0 {
                    pixels_changed |= self.draw_vector(new_x, new_y, control.ven_1);
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
                    pixels_changed |= self.set_pixel(x, y, PixelKind::Stored, size);
                    self.push_event(DisplayEvent::StoredMark {
                        position: DisplayPoint { x, y },
                        size,
                    });
                } else {
                    pixels_changed |= self.set_pixel(x, y, PixelKind::Cursor, 1);
                    pixels_changed |= self.set_pixel(x + 1, y + 1, PixelKind::Cursor, 1);
                    self.push_event(DisplayEvent::TransientCursorMark(DisplayPoint { x, y }));
                }
                self.adot_pending = false;
            }
            ScreenCommand::AlphaDot if control.vector_0 && !control.ven_1 => {
                self.adot_pending = true;
            }
            ScreenCommand::StartOfText | ScreenCommand::AlphaDot => {}
        }
        // Snapshot and native-image caches use this revision to detect framebuffer changes.
        if pixels_changed {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    // The reference phosphor callback removes cursor-level pixels and retains vector geometry.
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
    fn begin_erase(&mut self) {
        self.push_event(DisplayEvent::Erase);
        self.erase_retained_pixels.fill(0);
        self.pixels.fill(u8::MAX);
        self.revision = self.revision.wrapping_add(1);
    }

    fn complete_erase(&mut self) {
        // Remove the flash while retaining stored pixels written since erase began.
        self.pixels.copy_from_slice(&self.erase_retained_pixels);
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn take_events(&mut self) -> DisplayEventBatch {
        if self.events_dropped {
            self.events.clear();
            self.queued_cursor_marks.clear();
            self.events_dropped = false;
            return DisplayEventBatch {
                events: Vec::new(),
                dropped: true,
            };
        }
        let events = self.events.drain(..).collect();
        self.queued_cursor_marks.clear();
        DisplayEventBatch {
            events,
            dropped: false,
        }
    }

    pub(crate) fn clear_events(&mut self) {
        self.events.clear();
        self.queued_cursor_marks.clear();
        self.events_dropped = false;
    }

    pub(crate) fn take_screen_copies(&mut self) -> (Vec<Vec<u8>>, usize) {
        (
            std::mem::take(&mut self.screen_copies),
            std::mem::take(&mut self.dropped_screen_copies),
        )
    }

    fn push_event(&mut self, event: DisplayEvent) {
        if self.events_dropped {
            return;
        }
        match event {
            DisplayEvent::TransientCursorMark(position) => {
                if !self.queued_cursor_marks.insert(position) {
                    return;
                }
            }
            _ => self.queued_cursor_marks.clear(),
        }
        if self.events.len() == MAX_PENDING_DISPLAY_EVENTS {
            self.events.clear();
            self.queued_cursor_marks.clear();
            self.events_dropped = true;
            return;
        }
        self.events.push_back(event);
    }

    fn draw_vector(&mut self, x1: i32, y1: i32, ven_1: bool) -> bool {
        let from = DisplayPoint {
            x: self.x_da,
            y: self.y_da,
        };
        let to = DisplayPoint { x: x1, y: y1 };
        let mut x = self.x_da;
        let mut y = self.y_da;
        let dx = (x1 - x).abs();
        let dy = (y1 - y).abs();
        let step_x = if x < x1 { 1 } else { -1 };
        let step_y = if y < y1 { 1 } else { -1 };
        let mut error = dx - dy;
        let mut pixels_changed = false;
        let base_dot_size = if ven_1 { self.px_dot_size } else { 1 };

        loop {
            pixels_changed |= self.set_pixel(x, y, PixelKind::Stored, base_dot_size);
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

        if ven_1 {
            self.push_event(DisplayEvent::StoredVector {
                from,
                to,
                size: self.px_dot_size,
            });
        } else {
            self.push_event(DisplayEvent::TransientVector { from, to });
        }
        pixels_changed
    }

    fn set_pixel(&mut self, x: i32, y: i32, kind: PixelKind, size: usize) -> bool {
        // Convert the Tek y-up coordinate to the canvas y-down row, clipping fillRect at edges.
        let canvas_y = Self::HEIGHT as i32 - (y + 1);
        let mut pixels_changed = false;
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
                        if self.pixels[index] != self.pixel_cursor_intensity {
                            self.pixels[index] = self.pixel_cursor_intensity;
                            pixels_changed = true;
                        }
                    }
                    PixelKind::Cursor => {}
                    PixelKind::Stored => {
                        self.erase_retained_pixels[index] = self.pixel_store_intensity;
                        if self.pixels[index] != self.pixel_store_intensity {
                            self.pixels[index] = self.pixel_store_intensity;
                            pixels_changed = true;
                        }
                    }
                }
            }
        }
        pixels_changed
    }
}

#[cfg(test)]
#[path = "display/tests.rs"]
mod tests;

impl Default for CpuReferenceRenderer {
    fn default() -> Self {
        Self {
            pixels: vec![0; Self::WIDTH * Self::HEIGHT],
            erase_retained_pixels: vec![0; Self::WIDTH * Self::HEIGHT],
            revision: 0,
            x_da: 0,
            y_da: 0,
            adot_pending: false,
            granularity: DisplayGranularity::Coarse,
            px_dot_size: COARSE_DOT_SIZE,
            adot_size: COARSE_DOT_SIZE,
            pixel_store_intensity: COARSE_PIXEL_STORE_INTENSITY,
            pixel_cursor_intensity: COARSE_PIXEL_CURSOR_INTENSITY,
            events: VecDeque::new(),
            queued_cursor_marks: HashSet::new(),
            events_dropped: false,
            screen_copies: Vec::new(),
            dropped_screen_copies: 0,
        }
    }
}
