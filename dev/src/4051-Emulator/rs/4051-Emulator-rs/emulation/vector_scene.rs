//! Renderer-independent retained display state for optional vector presentation.
//!
//! The scene records semantic display events and their event-time granularity.
//! Vector strokes survive display refreshes while transient cursor marks do not;
//! vector glow remains a separate, age-limited presentation layer. Erase completion
//! clears the retained scene. Camera state and machine ScreenCopy data are outside it.

use std::mem::size_of;

use crate::machine::display::DisplayOperation;

use super::display::{DisplayEvent, DisplayEventBatch, DisplayGranularity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RecordedDisplayEvent {
    event: DisplayEvent,
    granularity: DisplayGranularity,
}

impl RecordedDisplayEvent {
    #[cfg(test)]
    fn granularity(&self) -> DisplayGranularity {
        self.granularity
    }
}

/// Approximate application-owned scene storage, excluding allocator metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SceneMemory {
    pub(super) current_bytes: usize,
    pub(super) peak_bytes: usize,
    pub(super) retained_geometry_events: usize,
}

impl SceneMemory {
    #[cfg(test)]
    fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    #[cfg(test)]
    fn peak_bytes(&self) -> usize {
        self.peak_bytes
    }

    #[cfg(test)]
    fn retained_geometry_events(&self) -> usize {
        self.retained_geometry_events
    }
}

/// Semantic display history retained while the optional vector feature is enabled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VectorScene {
    events: Vec<RecordedDisplayEvent>,
    erase_flash: bool,
    peak_reserved_bytes: usize,
}

impl VectorScene {
    /// Records one ordered operation and its already-decoded visible display events.
    pub(crate) fn apply_display_operation(
        &mut self,
        operation: DisplayOperation,
        events: &DisplayEventBatch,
        granularity: DisplayGranularity,
    ) -> Result<(), VectorSceneCaptureError> {
        if events.dropped {
            return Err(VectorSceneCaptureError::DisplayEventsDropped);
        }

        if matches!(operation, DisplayOperation::ScreenCopy) {
            return Ok(());
        }

        if matches!(operation, DisplayOperation::BeginErase) {
            // The delayed completion ends the flash; it must not clear geometry drawn afterward.
            self.events.clear();
            self.erase_flash = true;
        }

        for event in events.events.iter().copied() {
            match event {
                DisplayEvent::TransientCursorMark(_)
                | DisplayEvent::TransientVector { .. }
                | DisplayEvent::StoredMark { .. }
                | DisplayEvent::StoredVector { .. } => {
                    self.events
                        .try_reserve(1)
                        .map_err(|_| VectorSceneCaptureError::AllocationFailed)?;
                    self.events
                        .push(RecordedDisplayEvent { event, granularity });
                }
                DisplayEvent::Erase => self.erase_flash = true,
            }
        }

        match operation {
            DisplayOperation::BeginErase => self.erase_flash = true,
            DisplayOperation::CompleteErase => self.erase_flash = false,
            DisplayOperation::ScreenCommand { .. } | DisplayOperation::ScreenCopy => {}
        }

        self.update_memory_high_water();
        Ok(())
    }

    /// Drops refresh-only cursor marks while retaining every drawn vector stroke.
    pub(crate) fn refresh_display(&mut self) {
        self.events
            .retain(|recorded| !matches!(recorded.event, DisplayEvent::TransientCursorMark(_)));
    }

    /// Iterates retained geometry together with the granularity captured for each event.
    pub(crate) fn persistent_geometry_events(
        &self,
    ) -> impl Iterator<Item = (DisplayEvent, DisplayGranularity)> + '_ {
        self.events.iter().filter_map(|recorded| {
            matches!(
                recorded.event,
                DisplayEvent::StoredMark { .. }
                    | DisplayEvent::StoredVector { .. }
                    | DisplayEvent::TransientVector { .. }
            )
            .then_some((recorded.event, recorded.granularity))
        })
    }

    fn update_memory_high_water(&mut self) {
        self.peak_reserved_bytes = self.peak_reserved_bytes.max(self.reserved_bytes());
    }

    fn reserved_bytes(&self) -> usize {
        size_of::<Self>().saturating_add(
            self.events
                .capacity()
                .saturating_mul(size_of::<RecordedDisplayEvent>()),
        )
    }

    pub(super) fn memory_usage(&self) -> SceneMemory {
        SceneMemory {
            current_bytes: self.reserved_bytes(),
            peak_bytes: self.peak_reserved_bytes.max(self.reserved_bytes()),
            retained_geometry_events: self
                .events
                .iter()
                .filter(|recorded| {
                    matches!(
                        recorded.event,
                        DisplayEvent::StoredMark { .. }
                            | DisplayEvent::StoredVector { .. }
                            | DisplayEvent::TransientVector { .. }
                    )
                })
                .count(),
        }
    }

    #[cfg(test)]
    fn retained_geometry_events(&self) -> impl Iterator<Item = &RecordedDisplayEvent> {
        self.events.iter().filter(|recorded| {
            matches!(
                recorded.event,
                DisplayEvent::StoredMark { .. }
                    | DisplayEvent::StoredVector { .. }
                    | DisplayEvent::TransientVector { .. }
            )
        })
    }

    #[cfg(test)]
    fn transient_events(&self) -> impl Iterator<Item = &RecordedDisplayEvent> {
        self.events.iter().filter(|recorded| {
            matches!(
                recorded.event,
                DisplayEvent::TransientCursorMark(_) | DisplayEvent::TransientVector { .. }
            )
        })
    }

    #[cfg(test)]
    fn erase_flash(&self) -> bool {
        self.erase_flash
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VectorSceneCaptureError {
    AllocationFailed,
    DisplayEventsDropped,
}

#[cfg(test)]
mod tests;
