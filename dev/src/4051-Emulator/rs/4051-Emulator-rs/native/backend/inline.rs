//! Deterministic test adapter using production command and image preparation.

use super::*;

impl WorkerBackend {
    pub(crate) fn join(&mut self) -> Result<(), String> {
        self.request_shutdown();
        self.worker.take().map_or(Ok(()), |worker| {
            worker
                .join()
                .map_err(|_| "emulation worker panicked".to_owned())?
        })
    }
}

pub(crate) struct InlineBackend {
    pub(crate) state: EmulationState,
    presentation: DisplayPresentation,
}

#[cfg(test)]
impl InlineBackend {
    pub(crate) fn new(machine: Machine) -> Self {
        Self {
            state: EmulationState::new(machine),
            presentation: DisplayPresentation::default(),
        }
    }
}

#[cfg(test)]
impl Backend for InlineBackend {
    fn command(&mut self, command: Command) -> Result<(), String> {
        apply_command(&mut self.state, command)
    }

    fn snapshot(&mut self) -> Result<Option<PresentedSnapshot>, String> {
        Ok(Some(self.presentation.prepare(self.state.snapshot())))
    }

    fn advance(&mut self, now: Instant) {
        self.state.advance_to(now);
    }
}
