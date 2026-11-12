#![cfg(feature = "wasm")]

use std::{
    thread,
    time::{Duration, Instant},
};

use tek4051_emulator::wasm::{
    BrowserEmulator, BrowserGranularity, map_browser_key, map_browser_key_with_shift,
};

fn start_until_prompt(emulator: &mut BrowserEmulator) {
    emulator
        .set_running(true)
        .expect("the browser session should start");
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if emulator.advance().basic_prompt_ready() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("the ROM did not draw its BASIC cursor within the test safety bound");
}

// What: Translate browser logical keys and paired text through the shared Rust mapping.
// Why: Browser input must not maintain a second, drifting copy of Tek keyboard codes.
#[test]
fn browser_key_mapping_matches_shared_keyboard_rules() {
    // Arrange: Prepare paired layout-dependent punctuation and representative panel keys.
    let shifted_at = map_browser_key("q", Some("@".to_owned()))
        .expect("a single printable ASCII character should map");
    let german_asterisk = map_browser_key("+", Some("*".to_owned()))
        .expect("paired German-layout text should override the physical key");
    let german_plus = map_browser_key_with_shift("+", Some("+".to_owned()), false)
        .expect("unshifted German plus should use its host Shift state");
    let german_equals = map_browser_key_with_shift("=", Some("=".to_owned()), true)
        .expect("shifted German equals should use its host Shift state");
    let german_less_than = map_browser_key_with_shift("<", Some("<".to_owned()), false)
        .expect("unshifted German less-than should use its host Shift state");
    let german_semicolon = map_browser_key_with_shift(";", Some(";".to_owned()), true)
        .expect("shifted German semicolon should use its host Shift state");
    let backspace =
        map_browser_key("Backspace", None).expect("Backspace should map to a panel key");
    let function_alias =
        map_browser_key("F13", None).expect("F13 should retain its Home-key alias");
    let last_basic_function =
        map_browser_key("F10", None).expect("F10 should map to its panel code");
    let last_supported_function =
        map_browser_key("F17", None).expect("F17 should map to its panel code");

    // Act: Probe unsupported browser keys and invalid paired text.
    let unsupported_key = map_browser_key("ArrowLeft", None);
    let unsupported_functions = ["F11", "F12", "F18"]
        .map(|key| map_browser_key(key, None))
        .into_iter()
        .all(|mapping| mapping.is_none());
    let invalid_text = map_browser_key("a", Some("ab".to_owned()));
    let non_ascii_text = map_browser_key("a", Some("é".to_owned()));

    // Assert: Shared mappings preserve code/Shift behavior, while invalid input is rejected.
    assert_eq!(shifted_at.code(), b'@');
    assert!(shifted_at.invert_shift());
    assert!(shifted_at.text_input());
    assert_eq!(german_asterisk.code(), b'*');
    assert!(!german_asterisk.invert_shift());
    assert!(german_asterisk.text_input());
    assert_eq!(german_plus.code(), b';');
    assert!(german_plus.invert_shift());
    assert_eq!(german_equals.code(), b'=');
    assert!(!german_equals.invert_shift());
    assert_eq!(german_less_than.code(), b'<');
    assert!(german_less_than.invert_shift());
    assert_eq!(german_semicolon.code(), b';');
    assert!(german_semicolon.invert_shift());
    assert_eq!(backspace.code(), 0x08);
    assert!(!backspace.text_input());
    assert_eq!(function_alias.code(), 0x6a);
    assert_eq!(last_basic_function.code(), 0x69);
    assert_eq!(last_supported_function.code(), 0x76);
    assert!(unsupported_key.is_none());
    assert!(unsupported_functions);
    assert!(invalid_text.is_none());
    assert!(non_ascii_text.is_none());
}

// What: The browser wrapper starts from a reset, paused machine and exposes a full frame.
// Why: Browser code needs a stable initial state and explicit dimensions before drawing.
#[test]
fn new_emulator_exposes_paused_frame_and_registers() {
    // Arrange: construct the browser-facing session over the default machine.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");

    // Act: publish its initial immutable state.
    let snapshot = emulator.advance();

    // Assert: the session starts paused and its copied pixels match the logical canvas.
    assert!(!snapshot.running());
    assert_eq!(snapshot.width(), 1046);
    assert_eq!(snapshot.height(), 830);
    assert_eq!(
        snapshot.pixels().len(),
        (snapshot.width() * snapshot.height()) as usize
    );
    assert_eq!(snapshot.register_pc(), 0xbc4b);
    assert_eq!(snapshot.register_ccr(), 0xd0);
}

// What: Reject vector-scene capture when only the CPU renderer is available.
// Why: The optional feature must not silently activate without Extended WebGPU.
#[test]
fn vector_zoom_requires_extended_webgpu_and_stays_disabled_after_rejection() {
    // Arrange: create a browser session without initializing a WebGPU renderer.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");

    // Act: request vector zoom twice so an accidental enabled-state change is observable.
    let first_attempt = emulator.set_vector_zoom_enabled(true);
    let second_attempt = emulator.set_vector_zoom_enabled(true);

    // Assert: both attempts report the missing renderer instead of treating activation as a no-op.
    assert!(
        first_attempt
            .expect_err("CPU-only presentation must reject vector zoom")
            .contains("Extended WebGPU")
    );
    assert!(
        second_attempt
            .expect_err("a rejected activation must leave vector zoom disabled")
            .contains("Extended WebGPU")
    );
}

// What: Session controls update the public snapshot and reject unsupported speed indices.
// Why: Browser callers need validated control changes without inspecting Rust internals.
#[test]
fn controls_update_snapshot_and_validate_speed() {
    // Arrange: begin with the browser session's default speed and display mode.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");

    // Act: change supported controls, then request an unsupported speed.
    emulator.set_speed(2).expect("speed index 2 is supported");
    emulator
        .set_running(true)
        .expect("the session should resume");
    emulator
        .set_display_granularity(BrowserGranularity::Coarse)
        .expect("display mode changes do not produce host effects");
    let invalid_speed = emulator.set_speed(u32::MAX);
    let snapshot = emulator.snapshot();

    // Assert: the invalid request fails without replacing the last valid settings.
    assert!(invalid_speed.is_err());
    assert!(snapshot.running());
    assert_eq!(snapshot.speed_index(), 2);
    assert_eq!(snapshot.display_granularity(), BrowserGranularity::Coarse);
}

// What: A queued browser key remains observable until its sequenced tap reaches the keyboard.
// Why: Browser-side BASIC tests need a completion signal without guessing at key timing.
#[test]
fn pending_input_transitions_track_a_sequenced_browser_key() {
    // Arrange: Start the emulator at BASIC and translate one printable key.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    start_until_prompt(&mut emulator);
    let mapping =
        map_browser_key("a", Some("a".to_owned())).expect("printable browser text should map");

    // Act: Queue and release the key, then advance until the sequencer drains it.
    emulator
        .key_down(1, mapping.code(), mapping.invert_shift(), false, false)
        .expect("the browser key should be queued");
    emulator
        .key_up(1, false, false)
        .expect("the browser key should be released");
    let pending = emulator.pending_input_transitions();
    let deadline = Instant::now() + Duration::from_secs(2);
    while emulator.pending_input_transitions() != 0 && Instant::now() < deadline {
        emulator.advance();
        thread::sleep(Duration::from_millis(10));
    }
    let remaining = emulator.pending_input_transitions();
    emulator
        .release_input()
        .expect("test input should be released");

    // Assert: The queued tap was visible and eventually left no pending transitions.
    assert!(pending > 0);
    assert_eq!(remaining, 0);
}

// What: Run a finite BASIC program through the browser command sequencer.
// Why: Browser tests need each submitted line acknowledged by a fresh prompt, not guessed key delays.
#[test]
fn queued_basic_program_draws_and_returns_to_a_fresh_prompt() {
    // Arrange: Start the browser session at the ROM-detected BASIC prompt.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    start_until_prompt(&mut emulator);
    let starting_revision = emulator.snapshot().revision();

    // Act: Queue numbered source lines and let the sequencer submit them and run the program.
    emulator
        .queue_basic_program(vec![
            "100 PAGE".to_owned(),
            "110 MOVE 1,1".to_owned(),
            "120 DRAW 1022,778".to_owned(),
            "130 END".to_owned(),
        ])
        .expect("the running prompt should accept a BASIC program");
    let mut completed = false;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        let snapshot = emulator.advance();
        if emulator.pending_input_transitions() == 0 && snapshot.basic_prompt_ready() {
            completed = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let snapshot = emulator.snapshot();
    let pixels = snapshot.pixels();
    let width = snapshot.width() as usize;
    let height = snapshot.height() as usize;
    let diagonal_pixels = (17..=1038)
        .filter(|&x| {
            let expected_row: usize = 818 - ((x - 17) * 777 / 1021);
            (expected_row.saturating_sub(2)..=(expected_row + 2).min(height - 1))
                .any(|y| pixels[y * width + x] > 0)
        })
        .count();

    // Assert: The queued program returned to BASIC after producing visible output.
    assert!(
        completed,
        "the BASIC program should return to a fresh prompt"
    );
    assert!(snapshot.revision() > starting_revision);
    assert!(
        diagonal_pixels > 900,
        "the BASIC program should draw a full-screen diagonal, found {diagonal_pixels} matching columns"
    );
}

// What: Reset preserves run state, while Caps Lock remains visible to browser code.
// Why: Browser controls must observe lifecycle and keyboard settings through snapshots.
#[test]
fn reset_preserves_run_state_and_caps_lock_is_published() {
    // Arrange: create a paused session and change its Caps Lock state.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    emulator
        .set_caps_lock(true)
        .expect("Caps Lock changes do not produce host effects");
    emulator
        .set_running(true)
        .expect("the session should resume");

    // Act: reset while the session is running.
    emulator
        .reset()
        .expect("reset does not produce host effects");
    let snapshot = emulator.snapshot();

    // Assert: the reset session stays running and reports Caps Lock as enabled.
    assert!(snapshot.running());
    assert!(snapshot.caps_lock());
    assert_eq!(snapshot.register_pc(), 0xbc4b);
}

// What: Browser key identities govern releases, and focus loss cancels queued input.
// Why: A stale key-up must not cancel a tap, while a short matching tap still needs its scan interval.
#[test]
fn key_release_matches_identity_and_focus_loss_clears_queued_input() {
    // Arrange: wait for automatic cursor readiness before exercising accepted key identities.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    start_until_prompt(&mut emulator);
    assert!(!emulator.has_held_key());

    // Act: press one key, ignore a mismatched release, release its owner, then lose focus.
    emulator
        .key_down(7, b'A', false, false, false)
        .expect("a translated key press should be accepted");
    let remains_held = emulator.has_held_key();
    emulator
        .key_up(8, false, false)
        .expect("a stale key release should be ignored");
    let stale_release_kept_key = emulator.has_held_key();
    emulator
        .key_up(7, false, false)
        .expect("the owning key release should be accepted");
    let remains_held_after_owner_release = emulator.has_held_key();
    emulator
        .key_down(9, b'B', false, false, false)
        .expect("another translated key press should be accepted");
    emulator
        .release_input()
        .expect("focus loss should release emulated input");

    // Assert: a stale release changes nothing, the matching tap keeps its minimum hold, and blur clears it.
    assert!(remains_held);
    assert!(stale_release_kept_key);
    assert!(remains_held_after_owner_release);
    assert!(!emulator.has_held_key());
}

// What: Browser panel release preserves the minimum press duration and eventually releases the key.
// Why: Browser front-panel controls must not bypass the shared timed input sequence.
#[test]
fn panel_release_completes_after_minimum_hold() {
    // Arrange: wait for the machine-detected cursor before sending a panel key.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    start_until_prompt(&mut emulator);

    // Act: release while the key is held, then advance elapsed time in frame-sized intervals.
    let press_started = Instant::now();
    emulator
        .panel_press(b'R', false, false)
        .expect("a panel press should be accepted");
    let pressed = emulator.has_held_key();
    emulator
        .panel_release(false, false)
        .expect("a panel release should be accepted");
    let press_to_release_elapsed = press_started.elapsed();
    let held_during_minimum = emulator.has_held_key();
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(10));
        emulator.advance();
        if !emulator.has_held_key() {
            break;
        }
    }

    // Assert: an early release remains held through the minimum; ordinary advancement releases it.
    assert!(pressed);
    if press_to_release_elapsed < Duration::from_millis(50) {
        assert!(held_during_minimum);
    }
    assert!(!emulator.has_held_key());
}

// What: Keep an immediately released browser key held through the minimum input interval.
// Why: The browser may deliver keydown and keyup before the next emulation frame can scan it.
#[test]
fn browser_keyboard_tap_survives_an_immediate_release() {
    // Arrange: Wait for the detected cursor before sending a complete keyboard tap.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    start_until_prompt(&mut emulator);

    // Act: Send a complete key tap without advancing an emulation frame in between.
    emulator
        .key_down(1, b'A', false, false, false)
        .expect("the browser key press should be accepted");
    let press_started = Instant::now();
    emulator
        .key_up(1, false, false)
        .expect("the matching browser key release should be accepted");
    let press_to_release_elapsed = press_started.elapsed();
    let held_after_tap = emulator.has_held_key();
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(10));
        emulator.advance();
        if !emulator.has_held_key() {
            break;
        }
    }

    // Assert: The short tap stays owned until elapsed emulation time completes its minimum hold.
    if press_to_release_elapsed < Duration::from_millis(50) {
        assert!(held_after_tap);
    }
    assert!(!emulator.has_held_key());
}

// What: Focus loss clears a held key and all taps waiting behind it.
// Why: Queued browser input must not reappear after the surface loses ownership.
#[test]
fn focus_loss_cancels_held_and_queued_browser_keys() {
    // Arrange: Wait for the detected cursor, then queue two active/queued taps.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    start_until_prompt(&mut emulator);
    let input_started = Instant::now();
    for (key_id, code) in [(1, b'A'), (2, b'B')] {
        emulator
            .key_down(key_id, code, false, false, false)
            .expect("a browser key press should be accepted");
        emulator
            .key_up(key_id, false, false)
            .expect("the matching browser key release should be accepted");
    }
    let input_elapsed = input_started.elapsed();

    // Act: Report focus loss and watch later frames for a queued key to reappear.
    emulator
        .release_input()
        .expect("focus loss should release emulated input");
    let released_immediately = !emulator.has_held_key();
    let mut key_reappeared = false;
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(10));
        emulator.advance();
        key_reappeared |= emulator.has_held_key();
    }

    // Assert: neither the active key nor its queued successor returns after cancellation.
    assert!(released_immediately);
    if input_elapsed < Duration::from_millis(50) {
        assert!(!key_reappeared);
    }
}

// What: Reset clears active and queued browser taps from the previous machine state.
// Why: A reset must not replay stale user input into the newly initialized machine.
#[test]
fn reset_cancels_pending_browser_key_taps() {
    // Arrange: Wait for the detected cursor, then queue taps without advancing between events.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    start_until_prompt(&mut emulator);
    let input_started = Instant::now();
    for (key_id, code) in [(1, b'A'), (2, b'B')] {
        emulator
            .key_down(key_id, code, false, false, false)
            .expect("a browser key press should be accepted");
        emulator
            .key_up(key_id, false, false)
            .expect("the matching browser key release should be accepted");
    }
    let input_elapsed = input_started.elapsed();

    // Act: Reset and watch later frames for a queued key to reappear.
    emulator.reset().expect("the machine should reset");
    let released_immediately = !emulator.has_held_key();
    let mut key_reappeared = false;
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(10));
        emulator.advance();
        key_reappeared |= emulator.has_held_key();
    }

    // Assert: reset releases current ownership and leaves no stale key queued for replay.
    assert!(released_immediately);
    if input_elapsed < Duration::from_millis(50) {
        assert!(!key_reappeared);
    }
}
