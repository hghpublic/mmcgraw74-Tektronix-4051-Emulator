//! Run the session independently of native repaints and publish its latest view.
//! The worker alone owns the machine; a short snapshot lock never covers ROM
//! execution or rendering. The caller cancels and joins after the window closes.
//! A disconnected completion channel exposes unexpected worker termination.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use super::session::{Outcome, Session, TICK, TOTAL_TIMEOUT};

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;

#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) pixels: Vec<u8>,
    pub(super) revision: u64,
    pub(super) step: usize,
    pub(super) typed: String,
    pub(super) outcome: Option<Outcome>,
    pub(super) pc: u16,
    pub(super) busy: bool,
    pub(super) io: bool,
}

impl Snapshot {
    fn new(session: &mut Session) -> Self {
        let (pixels, revision) = {
            let display = session.display();
            (display.pixels().to_vec(), display.revision())
        };
        let indicators = session.machine.indicators();
        Self {
            pixels,
            revision,
            step: session.step,
            typed: session.typed.clone(),
            outcome: session.outcome.clone(),
            pc: session.machine.registers().pc,
            busy: indicators.busy,
            io: indicators.io,
        }
    }

    fn update(&mut self, session: &mut Session) {
        let indicators = session.machine.indicators();
        {
            let display = session.display();
            if self.revision != display.revision() {
                self.pixels.clone_from_slice(display.pixels());
                self.revision = display.revision();
            }
        }
        self.step = session.step;
        self.typed.clone_from(&session.typed);
        self.outcome.clone_from(&session.outcome);
        self.pc = session.machine.registers().pc;
        self.busy = indicators.busy;
        self.io = indicators.io;
    }
}

pub(super) struct SessionView {
    state: Arc<Mutex<Snapshot>>,
    completion: Receiver<()>,
}

impl SessionView {
    pub(super) fn snapshot(&self) -> Result<Snapshot, String> {
        // Check completion before reading state so a final publication cannot
        // race with disconnection and make a successful worker look abandoned.
        let finished = match self.completion.try_recv() {
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => true,
            Ok(()) => return Err("unexpected session completion message".to_owned()),
        };
        let snapshot = self
            .state
            .lock()
            .map_err(|_| "session snapshot lock is poisoned".to_owned())?
            .clone();
        if finished && snapshot.outcome.is_none() {
            return Err("session worker ended without an outcome".to_owned());
        }
        Ok(snapshot)
    }
}

pub(super) struct SessionWorker {
    pub(super) started: Instant,
    cancel: Arc<AtomicBool>,
    handle: JoinHandle<Result<(), String>>,
}

impl SessionWorker {
    pub(super) fn start(mut session: Session) -> Result<(Self, SessionView), String> {
        let state = Arc::new(Mutex::new(Snapshot::new(&mut session)));
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, completion) = mpsc::channel();
        let worker_state = Arc::clone(&state);
        let worker_cancel = Arc::clone(&cancel);
        let started = Instant::now();
        let handle = thread::Builder::new()
            .name(format!("{}-session", session.scenario.title))
            .spawn(move || {
                let _completion_lifetime = sender;
                while !worker_cancel.load(Ordering::Relaxed) && session.outcome.is_none() {
                    let tick_started = Instant::now();
                    if started.elapsed() >= TOTAL_TIMEOUT {
                        session.fail_wall_timeout();
                    } else {
                        session.tick();
                    }

                    worker_state
                        .lock()
                        .map_err(|_| "session snapshot lock is poisoned".to_owned())?
                        .update(&mut session);
                    if session.outcome.is_none() {
                        thread::sleep(TICK.saturating_sub(tick_started.elapsed()));
                    }
                }
                Ok(())
            })
            .map_err(|error| format!("cannot start session worker: {error}"))?;
        Ok((
            Self {
                started,
                cancel,
                handle,
            },
            SessionView { state, completion },
        ))
    }

    // All exit paths, including native-window startup failure, must join before
    // reporting a test result. A panic or publication failure remains failure.
    pub(super) fn finish(self) -> Result<(), String> {
        self.cancel.store(true, Ordering::Relaxed);
        self.handle.join().map_err(|panic| {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied());
            match message {
                Some(message) => format!("session worker panicked: {message}"),
                None => "session worker panicked with a non-string payload".to_owned(),
            }
        })?
    }
}
