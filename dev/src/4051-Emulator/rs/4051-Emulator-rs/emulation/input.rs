//! Framework-neutral keyboard ownership and timed application input.
//!
//! Native adapters supply Tek codes and Shift/Control values. Host key taps,
//! panel taps, and scripted commands share 50 ms press and 30 ms release
//! phases so quick down/up pairs remain visible to the emulated keyboard. The
//! sequencer advances runtime between transitions. Platform key types, layout,
//! pointer hit testing and widgets stay in adapters.

use std::{collections::VecDeque, time::Duration};

use super::runtime::{HostRuntime, HostRuntimeError};
use crate::machine::{KeyboardModifier, Machine};

const MINIMUM_PRESS: Duration = Duration::from_millis(50);
const MINIMUM_RELEASE: Duration = Duration::from_millis(30);
const AUTO_LOAD_KEYS: &[u8] = b"FIND@5:1\rOLD@5:\rRUN\r";

#[cfg(test)]
#[path = "input/tests.rs"]
mod tests;

/// Modifier lines understood by the emulated keyboard, without host shortcuts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub shift: bool,
    pub control: bool,
}

impl KeyModifiers {
    pub const NONE: Self = Self {
        shift: false,
        control: false,
    };
}

/// An adapter-assigned identity, independent of the translated Tek key code.
///
/// Use distinct IDs for different logical keys and reuse each key's ID for its
/// press and release; ownership is decided only when the worker accepts input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualKeyId(pub u64);

/// Ordered input effects after native key/text pairing and pointer hit testing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputAction {
    Modifiers(KeyModifiers),
    ManualPress {
        key: ManualKeyId,
        code: u8,
        invert_shift: bool,
        modifiers: KeyModifiers,
    },
    ManualRelease(ManualKeyId),
    /// Queues a host key so short presses reach the emulated keyboard.
    SequencedManualPress {
        key: ManualKeyId,
        code: u8,
        invert_shift: bool,
        modifiers: KeyModifiers,
    },
    SequencedManualRelease(ManualKeyId),
    PanelPress {
        code: u8,
        modifiers: KeyModifiers,
    },
    PanelRelease(KeyModifiers),
}

/// One translated frame; reset clears prior input but still permits new panel actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputFrame {
    pub actions: Vec<InputAction>,
    pub modifiers: KeyModifiers,
    pub focused: bool,
    pub reset_input: bool,
}

impl InputFrame {
    /// Releases all input, including a script, before any subsequent elapsed-time catch-up.
    pub fn unfocused() -> Self {
        Self {
            actions: Vec::new(),
            modifiers: KeyModifiers::NONE,
            focused: false,
            reset_input: false,
        }
    }

    pub fn apply(
        &self,
        sequence: &mut InputSequence,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        if !self.focused {
            keyboard.release_manual(machine);
            keyboard.release_panel(machine);
            keyboard.apply_modifiers(machine, KeyModifiers::NONE);
            sequence.cancel(keyboard, machine);
            return;
        }
        if self.reset_input {
            sequence.cancel(keyboard, machine);
            keyboard.release_manual(machine);
            keyboard.apply_modifiers(machine, KeyModifiers::NONE);
        }
        for action in &self.actions {
            match *action {
                InputAction::Modifiers(modifiers) => keyboard.apply_modifiers(machine, modifiers),
                InputAction::ManualPress {
                    key,
                    code,
                    invert_shift,
                    modifiers,
                } => {
                    sequence.cancel(keyboard, machine);
                    keyboard.press_manual(machine, key, code, invert_shift, modifiers);
                }
                InputAction::ManualRelease(key) => {
                    if matches!(keyboard.owner, KeyOwner::Manual { key: held, .. } if held == key) {
                        keyboard.release_manual(machine);
                    }
                }
                InputAction::SequencedManualPress {
                    key,
                    code,
                    invert_shift,
                    modifiers,
                } => sequence.press_manual(key, code, invert_shift, modifiers, keyboard, machine),
                InputAction::SequencedManualRelease(key) => {
                    sequence.release_manual(key, self.modifiers, keyboard, machine)
                }
                InputAction::PanelPress { code, modifiers } => {
                    sequence.press(code, modifiers, keyboard, machine)
                }
                InputAction::PanelRelease(modifiers) => {
                    sequence.release(modifiers, keyboard, machine)
                }
            }
        }
        keyboard.apply_modifiers(machine, self.modifiers);
    }
}

#[derive(Default)]
enum KeyOwner {
    #[default]
    None,
    Manual {
        key: ManualKeyId,
        invert_shift: bool,
        held: Option<KeyModifiers>,
    },
    Sequence {
        held: Option<KeyModifiers>,
    },
}

/// Applies translated keys while keeping manual and sequenced ownership exclusive.
#[derive(Default)]
pub struct KeyboardState {
    owner: KeyOwner,
}

impl KeyboardState {
    pub fn is_idle(&self) -> bool {
        matches!(self.owner, KeyOwner::None)
    }

    /// Applies a host-translated key; the adapter cancels any pending sequence first.
    pub fn press_manual(
        &mut self,
        machine: &mut Machine,
        key: ManualKeyId,
        code: u8,
        invert_shift: bool,
        modifiers: KeyModifiers,
    ) {
        machine.keyboard_press(code);
        self.owner = KeyOwner::Manual {
            key,
            invert_shift,
            held: None,
        };
        self.apply_modifiers(machine, modifiers);
    }

    /// Releases only manual ownership; a late host release cannot release a panel key.
    pub fn release_manual(&mut self, machine: &mut Machine) {
        if matches!(self.owner, KeyOwner::Manual { .. }) {
            machine.keyboard_release();
            self.owner = KeyOwner::None;
        }
    }

    fn set_panel_key(&mut self, machine: &mut Machine, code: Option<u8>, modifiers: KeyModifiers) {
        if let Some(code) = code {
            machine.keyboard_press(code);
            self.owner = KeyOwner::Sequence { held: None };
        } else {
            self.release_panel(machine);
        }
        self.apply_modifiers(machine, modifiers);
    }

    pub fn release_panel(&mut self, machine: &mut Machine) {
        if matches!(self.owner, KeyOwner::Sequence { .. }) {
            machine.keyboard_release();
            self.owner = KeyOwner::None;
        }
    }

    fn hold_manual_modifiers(
        &mut self,
        machine: &mut Machine,
        key: ManualKeyId,
        held: KeyModifiers,
        current: KeyModifiers,
    ) {
        if let KeyOwner::Manual {
            key: held_key,
            held: saved,
            ..
        } = &mut self.owner
        {
            if *held_key == key {
                *saved = Some(held);
                self.apply_modifiers(machine, current);
            }
        }
    }

    fn hold_panel_modifiers(
        &mut self,
        machine: &mut Machine,
        held: KeyModifiers,
        current: KeyModifiers,
    ) {
        if let KeyOwner::Sequence { held: saved } = &mut self.owner {
            *saved = Some(held);
            self.apply_modifiers(machine, current);
        }
    }

    pub fn apply_modifiers(&self, machine: &mut Machine, current: KeyModifiers) {
        let (modifiers, invert_shift) = match self.owner {
            KeyOwner::Sequence { held: Some(held) } => (held, false),
            KeyOwner::Manual {
                invert_shift, held, ..
            } => (held.unwrap_or(current), invert_shift),
            _ => (current, false),
        };
        machine.set_keyboard_modifier(KeyboardModifier::Shift, modifiers.shift ^ invert_shift);
        machine.set_keyboard_modifier(KeyboardModifier::Control, modifiers.control);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PressId {
    Panel(u64),
    Manual {
        serial: u64,
        key: ManualKeyId,
        invert_shift: bool,
    },
    Script(u64),
}

struct Press {
    id: PressId,
    code: u8,
    modifiers: KeyModifiers,
    released: bool,
}

impl Press {
    fn hold_modifiers(
        &self,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
        modifiers: KeyModifiers,
    ) {
        match self.id {
            PressId::Manual { key, .. } => {
                keyboard.hold_manual_modifiers(machine, key, modifiers, modifiers)
            }
            PressId::Panel(_) | PressId::Script(_) => {
                keyboard.hold_panel_modifiers(machine, modifiers, modifiers)
            }
        }
    }
}

struct ActivePress {
    press: Press,
    remaining: Duration,
}

#[derive(Clone, Copy)]
struct ScriptKey {
    code: u8,
    modifiers: KeyModifiers,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoLoadState {
    #[default]
    Idle,
    Typing,
    Sent,
    Cancelled,
}

/// Serializes browser/panel key taps and scripts without native events or widget geometry.
#[derive(Default)]
pub struct InputSequence {
    held_panel_press: Option<PressId>,
    // Keep distinct host key identities until each matching key-up arrives.
    held_manual_presses: Vec<PressId>,
    next_id: u64,
    active: Option<ActivePress>,
    queued: VecDeque<Press>,
    pending_program_commands: VecDeque<Vec<ScriptKey>>,
    program_load_active: bool,
    release_remaining: Duration,
    release_owner: Option<PressId>,
    auto_load_state: AutoLoadState,
}

impl InputSequence {
    pub fn auto_load_state(&self) -> AutoLoadState {
        self.auto_load_state
    }

    pub(crate) fn program_load_active(&self) -> bool {
        self.program_load_active
    }

    /// True between lines after the preceding command's key sequence fully releases.
    pub(crate) fn waiting_for_program_prompt(&self) -> bool {
        self.program_load_active
            && !self.pending_program_commands.is_empty()
            && self.active.is_none()
            && self.queued.is_empty()
            && self.release_remaining.is_zero()
    }

    pub(crate) fn pending_key_transitions(&self) -> usize {
        let queued = self
            .queued
            .iter()
            .map(|press| 1 + usize::from(press.released))
            .sum::<usize>();
        let pending_program = self
            .pending_program_commands
            .iter()
            .map(|command| 2 * command.len())
            .sum::<usize>();
        queued
            + pending_program
            + usize::from(
                self.active
                    .as_ref()
                    .is_some_and(|active| active.press.released),
            )
    }

    /// Queues fixed-modifier taps without displacing held or pending manual input.
    pub fn queue_auto_load(
        &mut self,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) -> Result<(), &'static str> {
        self.queue_keys(AUTO_LOAD_KEYS, keyboard, machine)
    }

    pub fn queue_program_load(
        &mut self,
        file_number: u8,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) -> Result<(), &'static str> {
        self.queue_program_load_with_loader(file_number, "OLD@5:", keyboard, machine)
    }

    /// Queues a binary program load through the R05 `BOLD` routine.
    pub fn queue_binary_program_load(
        &mut self,
        file_number: u8,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) -> Result<(), &'static str> {
        self.queue_program_load_with_loader(file_number, "CALL \"BOLD\",5", keyboard, machine)
    }

    pub(crate) fn queue_basic_program(
        &mut self,
        lines: Vec<String>,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) -> Result<(), &'static str> {
        if lines.is_empty() {
            return Err("BASIC program is empty");
        }
        let mut commands = Vec::with_capacity(lines.len() + 1);
        for line in lines {
            if line.is_empty() || line.contains('\r') || line.contains('\n') {
                return Err("BASIC program lines must be nonempty single lines");
            }
            // The ROM line editor uses TAB keystrokes between BASIC tokens.
            commands.push(format!("{}\r", line.replace(' ', "\x10")));
        }
        commands.push("RUN\r".to_owned());
        self.queue_program_commands(commands, keyboard, machine)
    }

    fn queue_program_load_with_loader(
        &mut self,
        file_number: u8,
        loader_command: &str,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) -> Result<(), &'static str> {
        self.queue_program_commands(
            [
                format!("FIND@5:{file_number}\r"),
                format!("{loader_command}\r"),
                "RUN\r".to_owned(),
            ],
            keyboard,
            machine,
        )
    }

    fn queue_program_commands(
        &mut self,
        commands: impl IntoIterator<Item = String>,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) -> Result<(), &'static str> {
        if self.keyboard_is_busy(keyboard) {
            return Err("keyboard input is busy");
        }
        let mut commands = commands
            .into_iter()
            .map(|command| Self::map_program_command(&command))
            .collect::<Result<VecDeque<_>, _>>()?;
        let first = commands
            .pop_front()
            .ok_or("program load sequence is empty")?;
        self.pending_program_commands = commands;
        self.program_load_active = true;
        self.queue_program_keys_inner(&first, keyboard, machine);
        Ok(())
    }

    fn map_program_command(command: &str) -> Result<Vec<ScriptKey>, &'static str> {
        command
            .chars()
            .map(|character| {
                if character == '\r' || character == '\x10' {
                    return Ok(ScriptKey {
                        code: character as u8,
                        modifiers: KeyModifiers::NONE,
                    });
                }
                let mapping = super::keyboard::map_text_character_with_host_shift(character, false)
                    .ok_or("unsupported program-load character")?;
                Ok(ScriptKey {
                    code: mapping.code,
                    modifiers: KeyModifiers {
                        shift: mapping.invert_shift,
                        control: false,
                    },
                })
            })
            .collect()
    }

    pub(crate) fn resume_program_load_if_ready(
        &mut self,
        prompt_ready: bool,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) -> bool {
        if !prompt_ready
            || self.auto_load_state != AutoLoadState::Typing
            || self.active.is_some()
            || !self.queued.is_empty()
            || !self.release_remaining.is_zero()
            || self.pending_program_commands.is_empty()
        {
            return false;
        }
        let command = self
            .pending_program_commands
            .pop_front()
            .expect("a queued program-load command must exist when resuming");
        self.queue_program_keys_inner(&command, keyboard, machine);
        true
    }

    fn keyboard_is_busy(&self, keyboard: &KeyboardState) -> bool {
        self.active.is_some()
            || !self.queued.is_empty()
            || !self.pending_program_commands.is_empty()
            || self.held_panel_press.is_some()
            || !self.held_manual_presses.is_empty()
            || !self.release_remaining.is_zero()
            || !keyboard.is_idle()
    }

    fn queue_keys(
        &mut self,
        keys: &[u8],
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) -> Result<(), &'static str> {
        if self.keyboard_is_busy(keyboard) {
            return Err("keyboard input is busy");
        }
        self.queue_keys_inner(keys, keyboard, machine);
        Ok(())
    }

    fn queue_keys_inner(
        &mut self,
        keys: &[u8],
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        self.enqueue_script_keys(
            keys.iter().copied().map(|code| ScriptKey {
                code,
                modifiers: KeyModifiers::NONE,
            }),
            keyboard,
            machine,
        );
    }

    fn queue_program_keys_inner(
        &mut self,
        keys: &[ScriptKey],
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        self.enqueue_script_keys(keys.iter().copied(), keyboard, machine);
    }

    fn enqueue_script_keys(
        &mut self,
        keys: impl IntoIterator<Item = ScriptKey>,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        for key in keys {
            let id = PressId::Script(self.next_id);
            self.next_id = self.next_id.wrapping_add(1);
            self.queued.push_back(Press {
                id,
                code: key.code,
                modifiers: key.modifiers,
                released: true,
            });
        }
        self.auto_load_state = AutoLoadState::Typing;
        self.start_next(keyboard, machine, KeyModifiers::NONE);
    }

    /// Begins a translated panel press; repeated downs before release are ignored.
    pub fn press(
        &mut self,
        code: u8,
        modifiers: KeyModifiers,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        if self.held_panel_press.is_some() {
            return;
        }
        if self.has_manual_work() || self.auto_load_state == AutoLoadState::Typing {
            self.cancel(keyboard, machine);
        }
        let id = PressId::Panel(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.held_panel_press = Some(id);
        self.queued.push_back(Press {
            id,
            code,
            modifiers,
            released: false,
        });
        self.start_next(keyboard, machine, modifiers);
    }

    fn press_manual(
        &mut self,
        key: ManualKeyId,
        code: u8,
        invert_shift: bool,
        modifiers: KeyModifiers,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        if self
            .held_manual_presses
            .iter()
            .any(|press| matches!(press, PressId::Manual { key: held, .. } if *held == key))
        {
            return;
        }

        if self.has_non_manual_work() || self.auto_load_state == AutoLoadState::Typing {
            self.cancel(keyboard, machine);
        }
        let id = PressId::Manual {
            serial: self.next_id,
            key,
            invert_shift,
        };
        self.next_id = self.next_id.wrapping_add(1);
        self.held_manual_presses.push(id);
        self.queued.push_back(Press {
            id,
            code,
            modifiers,
            released: false,
        });
        self.start_next(keyboard, machine, modifiers);
    }

    /// Captures release-time modifiers until the minimum press has elapsed.
    pub fn release(
        &mut self,
        modifiers: KeyModifiers,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        let Some(id @ PressId::Panel(_)) = self.held_panel_press else {
            return;
        };
        self.held_panel_press = None;
        self.release_press(id, modifiers, keyboard, machine);
    }

    fn release_manual(
        &mut self,
        key: ManualKeyId,
        modifiers: KeyModifiers,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        let Some(index) = self
            .held_manual_presses
            .iter()
            .position(|press| matches!(press, PressId::Manual { key: held, .. } if *held == key))
        else {
            return;
        };
        let id = self.held_manual_presses.remove(index);
        self.release_press(id, modifiers, keyboard, machine);
    }

    fn release_press(
        &mut self,
        id: PressId,
        modifiers: KeyModifiers,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
    ) {
        if let Some(active) = self.active.as_mut().filter(|active| active.press.id == id) {
            active.press.released = true;
            active.press.modifiers = modifiers;
            active.press.hold_modifiers(keyboard, machine, modifiers);
            if active.remaining.is_zero() {
                self.finish_press(keyboard, machine, modifiers);
            }
        } else if let Some(press) = self.queued.iter_mut().find(|press| press.id == id) {
            press.released = true;
            press.modifiers = modifiers;
        }
    }

    pub fn cancel(&mut self, keyboard: &mut KeyboardState, machine: &mut Machine) {
        if self.auto_load_state == AutoLoadState::Typing {
            self.auto_load_state = AutoLoadState::Cancelled;
        }
        self.held_panel_press = None;
        self.held_manual_presses.clear();
        self.active = None;
        self.queued.clear();
        self.pending_program_commands.clear();
        self.program_load_active = false;
        self.release_remaining = Duration::ZERO;
        self.release_owner = None;
        keyboard.release_manual(machine);
        keyboard.set_panel_key(machine, None, KeyModifiers::NONE);
        keyboard.apply_modifiers(machine, KeyModifiers::NONE);
    }

    /// Advances runtime at key boundaries; erase callbacks still consume an error's remaining tail.
    pub fn advance(
        &mut self,
        elapsed: Duration,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
        runtime: &mut HostRuntime,
        modifiers: KeyModifiers,
    ) -> Result<(), HostRuntimeError> {
        let mut remaining = elapsed;
        while !remaining.is_zero() {
            self.start_next(keyboard, machine, modifiers);
            let step = if let Some(active) = &self.active {
                if active.remaining.is_zero() {
                    remaining
                } else {
                    remaining.min(active.remaining)
                }
            } else if !self.release_remaining.is_zero() {
                remaining.min(self.release_remaining)
            } else {
                remaining
            };
            let result = runtime.advance(machine, step);
            remaining -= step;
            if let Err(error) = result {
                runtime.advance_erase_callbacks(machine, remaining);
                return Err(error);
            }
            if let Some(active) = &mut self.active {
                active.remaining = active.remaining.saturating_sub(step);
                if active.remaining.is_zero() && active.press.released {
                    self.finish_press(keyboard, machine, modifiers);
                }
            } else {
                self.release_remaining = self.release_remaining.saturating_sub(step);
                if self.release_remaining.is_zero() {
                    self.release_owner = None;
                }
            }
            if self.auto_load_state == AutoLoadState::Typing
                && self.active.is_none()
                && self.queued.is_empty()
                && self.release_remaining.is_zero()
                && self.pending_program_commands.is_empty()
            {
                self.auto_load_state = AutoLoadState::Sent;
                self.program_load_active = false;
            }
        }
        self.start_next(keyboard, machine, modifiers);
        Ok(())
    }

    fn start_next(
        &mut self,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
        current_modifiers: KeyModifiers,
    ) {
        if self.active.is_some() || !self.release_remaining.is_zero() {
            return;
        }
        if let Some(press) = self.queued.pop_front() {
            let modifiers = if press.released {
                press.modifiers
            } else {
                current_modifiers
            };
            match press.id {
                PressId::Manual {
                    key, invert_shift, ..
                } => keyboard.press_manual(machine, key, press.code, invert_shift, modifiers),
                PressId::Panel(_) | PressId::Script(_) => {
                    keyboard.set_panel_key(machine, Some(press.code), modifiers)
                }
            }
            if press.released {
                press.hold_modifiers(keyboard, machine, press.modifiers);
            }
            self.active = Some(ActivePress {
                press,
                remaining: MINIMUM_PRESS,
            });
        }
    }

    fn finish_press(
        &mut self,
        keyboard: &mut KeyboardState,
        machine: &mut Machine,
        modifiers: KeyModifiers,
    ) {
        if let Some(active) = self.active.take() {
            self.release_owner = Some(active.press.id);
            match active.press.id {
                PressId::Manual { .. } => keyboard.release_manual(machine),
                PressId::Panel(_) | PressId::Script(_) => {
                    keyboard.set_panel_key(machine, None, modifiers)
                }
            }
        }
        keyboard.apply_modifiers(machine, modifiers);
        self.release_remaining = MINIMUM_RELEASE;
    }

    fn has_manual_work(&self) -> bool {
        !self.held_manual_presses.is_empty()
            || self
                .active
                .as_ref()
                .is_some_and(|active| matches!(active.press.id, PressId::Manual { .. }))
            || self
                .queued
                .iter()
                .any(|press| matches!(press.id, PressId::Manual { .. }))
            || matches!(self.release_owner, Some(PressId::Manual { .. }))
    }

    fn has_non_manual_work(&self) -> bool {
        self.held_panel_press.is_some()
            || self
                .active
                .as_ref()
                .is_some_and(|active| !matches!(active.press.id, PressId::Manual { .. }))
            || self
                .queued
                .iter()
                .any(|press| !matches!(press.id, PressId::Manual { .. }))
            || self
                .release_owner
                .is_some_and(|press| !matches!(press, PressId::Manual { .. }))
    }
}
