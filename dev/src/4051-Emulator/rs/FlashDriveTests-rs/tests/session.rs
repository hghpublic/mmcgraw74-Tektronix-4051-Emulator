#[path = "support/session.rs"]
mod session;

use session::Session;

// What: Advance the extracted real-ROM session through boot into the first scripted key.
// Why: Reusing the driver without eframe must preserve stage transitions and keyboard entry.
#[test]
fn windowless_session_boots_and_starts_find() {
    // Arrange: Import the unchanged SimpleDemo set through the same constructor as the GUI.
    let mut session = Session::new("simple_demo").expect("SimpleDemo assets must import");

    // Act: Supply bounded logical ticks without a native window, then begin command entry.
    for _ in 0..200 {
        session.tick();
        if session.step != 0 || session.outcome.is_some() {
            break;
        }
    }
    assert!(session.outcome.is_none(), "{:?}", session.outcome);
    assert_eq!(session.step, 1, "ROM must reach the FIND stage");
    session.tick();

    // Assert: The imported session reached command readiness and emitted its first FIND character.
    assert_eq!(session.typed, "F");
    assert!(session.outcome.is_none(), "{:?}", session.outcome);
}

// What: Trigger a session deadline and attempt further logical ticks.
// Why: A windowless caller must receive the same failure and preserve the terminal machine frame.
#[test]
fn deadline_failure_freezes_the_session() {
    // Arrange: Start a real session and capture its machine/frame state before the deadline.
    let mut session = Session::new("simple_demo").expect("SimpleDemo assets must import");
    let registers = session.machine.registers();
    let revision = session.display().revision();

    // Act: Report the caller's wall deadline and attempt to continue the driver.
    session.fail_wall_timeout();
    let outcome = session.outcome.clone();
    for _ in 0..10 {
        session.tick();
    }

    // Assert: The visible stage diagnostic survives and no CPU or framebuffer update follows failure.
    let error = session.outcome.as_ref().unwrap().as_ref().unwrap_err();
    assert!(error.contains("Wait for ROM command readiness"), "{error}");
    assert_eq!(session.outcome, outcome);
    assert_eq!(session.machine.registers(), registers);
    assert_eq!(session.display().revision(), revision);
    assert_eq!(session.step, 0);
    assert!(session.typed.is_empty());
}

// What: Import each supplied smoke set and execute its selected program's FIND through the ROM.
// Why: Each target must use its own complete assets and finish the real command, not just emit F.
#[test]
fn new_flash_drive_sets_boot_and_complete_selected_find() {
    // Arrange: Use complete unchanged fixtures, their duplicates and F35's file-200 entry.
    for (name, count, command) in [
        ("battlestar", 91, b"FIND@5:1\r".as_slice()),
        ("star_wars", 11, b"FIND@5:1\r".as_slice()),
        ("benchmark", 8, b"FIND@5:1\r".as_slice()),
        ("asteroids", 81, b"FIND@5:1\r".as_slice()),
        ("f35", 28, b"FIND@5:200\r".as_slice()),
        ("fractals", 3, b"FIND@5:1\r".as_slice()),
        ("pictures", 41, b"FIND@5:1\r".as_slice()),
        ("svg2fg", 83, b"FIND@5:1\r".as_slice()),
        ("spacelab", 11, b"FIND@5:1\r".as_slice()),
        ("michlanglo", 11, b"FIND@5:1\r".as_slice()),
    ] {
        let mut session = Session::new(name).expect("all supplied files must import");
        assert_eq!(session.imported_files, count, "{name}");

        // Act: Drive real-ROM boot without a window, then execute the complete selected FIND command.
        for _ in 0..200 {
            session.tick();
            if session.step != 0 || session.outcome.is_some() {
                break;
            }
        }
        session.tick();
        assert!(session.outcome.is_none(), "{name}: {:?}", session.outcome);
        assert_eq!(session.step, 1, "{name}");
        assert_eq!(session.typed, "F", "{name}");
        assert_eq!(session.scenario.steps[1].keys, command, "{name}");
        for _ in 0..500 {
            session.tick();
            if session.step >= 2 || session.outcome.is_some() {
                break;
            }
        }

        // Assert: ROM command readiness has accepted the full FIND and advanced to loading that entry.
        assert!(session.outcome.is_none(), "{name}: {:?}", session.outcome);
        assert_eq!(session.step, 2, "{name}: selected FIND did not complete");
    }
}

// What: Request a Flash Drive scenario that has no registered definition.
// Why: Invalid selection must report an error rather than running the wrong supplied program.
#[test]
fn unknown_scenario_is_an_explicit_error() {
    // Arrange: Select a nonexistent scenario instead of a registered asset directory.
    let name = "missing";
    // Act: Attempt construction before any ROM worker or native window starts.
    let error = Session::new(name)
        .err()
        .expect("unknown scenario must fail");
    // Assert: The diagnostic identifies the rejected selection without a fallback session.
    assert!(
        error.contains("unknown Flash Drive scenario: missing"),
        "{error}"
    );
}

// What: Run Benchmark, StarWars and file-200 F35 through all four real-ROM startup-smoke stages.
// Why: Verify real selected-file loading/execution and INPUT/drawing results, not just configured keys.
#[test]
fn benchmark_star_wars_and_f35_complete_their_startup_smoke_sequences() {
    // Arrange: Import the unchanged file-1 sets and F35's selected file-200 drawing entry.
    for name in ["benchmark", "star_wars", "f35"] {
        let mut session = Session::new(name).expect("supplied assets must import");

        // Act: Supply bounded logical ticks through boot, FIND, OLD and RUN without repaint callbacks.
        for _ in 0..4_000 {
            session.tick();
            if session.outcome.is_some() {
                break;
            }
        }

        // Assert: The input/drawing smoke result completed successfully rather than timing out.
        assert_eq!(session.outcome, Some(Ok(())), "{name}");
        assert_eq!(session.step, 4, "{name}");
        if name == "benchmark" {
            let indicators = session.machine.indicators();
            assert!(indicators.io, "Benchmark must finish at its INPUT prompt");
            assert!(
                indicators.busy,
                "Benchmark's INPUT retains BUSY, not command-idle state"
            );
        }
    }
}
