//! Renders the Flash Drive directory and translates its navigation into UI actions.
//!
//! The parent application owns prompt and selection state, then decides whether
//! the returned selection should be sent to the emulation session.

use eframe::egui;
use tek4051_emulator::machine::gpib::FlashDriveFileInfo;

#[derive(Default)]
pub(super) struct Action {
    pub(super) keyboard_activation_owned: bool,
    pub(super) list_focused: bool,
    pub(super) launch_file_number: Option<u8>,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    directory: &[FlashDriveFileInfo],
    runtime_running: bool,
    basic_prompt_ready: bool,
    input_notice: Option<&str>,
    list_focused: &mut bool,
    selected_file_number: &mut Option<u8>,
) -> Action {
    ui.heading("Flash Drive directory");
    ui.label("Program launches wait for a detected BASIC cursor or INPUT prompt.");
    ui.label(if !runtime_running {
        "Start the emulator before launching a program."
    } else if basic_prompt_ready {
        "BASIC prompt detected; program launch is available."
    } else {
        "Waiting for the BASIC cursor or INPUT prompt; host keyboard input still reaches the machine."
    });
    if let Some(notice) = input_notice {
        ui.colored_label(egui::Color32::LIGHT_RED, notice);
    }
    let mut action = Action::default();
    let was_list_focused = *list_focused;
    let (up, down, enter, tab, pointer_pressed) = ui.ctx().input(|input| {
        let key_pressed = |key| {
            input.raw.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: event_key,
                        pressed: true,
                        ..
                    } if *event_key == key
                )
            })
        };
        (
            key_pressed(egui::Key::ArrowUp),
            key_pressed(egui::Key::ArrowDown),
            key_pressed(egui::Key::Enter),
            key_pressed(egui::Key::Tab),
            input.pointer.any_pressed(),
        )
    });
    let list_navigation_key = up || down || enter;
    if was_list_focused && tab {
        *list_focused = false;
    }
    ui.label("Use Up/Down to move the selection.");
    if selected_file_number
        .is_some_and(|number| !directory.iter().any(|file| file.number == number))
    {
        *selected_file_number = None;
    }
    if directory.is_empty() {
        *list_focused = false;
        ui.label("No stored files");
        return action;
    }
    ui.label(format!("{} stored file(s)", directory.len()));
    let mut row_responses = Vec::with_capacity(directory.len());
    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("flash_drive_directory_rows")
            .striped(true)
            .show(ui, |ui| {
                for heading in ["Number", "Type", "Usage", "Bytes", "Description"] {
                    ui.strong(heading);
                }
                ui.end_row();
                for file in directory {
                    let number = ui.selectable_label(
                        *selected_file_number == Some(file.number),
                        file.number.to_string(),
                    );
                    if number.clicked() {
                        *selected_file_number = Some(file.number);
                        number.request_focus();
                    }
                    if number.double_clicked() {
                        *selected_file_number = Some(file.number);
                        action.launch_file_number = Some(file.number);
                    }
                    let row_focused = number.has_focus() || number.clicked();
                    if row_focused {
                        *list_focused = true;
                        if tab && number.has_focus() {
                            *selected_file_number = Some(file.number);
                        }
                    } else if number.lost_focus() && (tab || pointer_pressed) {
                        *list_focused = false;
                    }
                    action.keyboard_activation_owned |= row_focused;
                    row_responses.push(number.clone());
                    match file.file_type {
                        Some(file_type) => {
                            ui.label(file_type);
                        }
                        None => {
                            ui.colored_label(egui::Color32::LIGHT_RED, "Missing type");
                        }
                    }
                    match file.usage {
                        Some(usage) => {
                            ui.label(usage);
                        }
                        None if matches!(file.file_type, Some("NEW" | "LAST")) => {
                            ui.label("-");
                        }
                        None => {
                            ui.colored_label(egui::Color32::LIGHT_RED, "Missing usage");
                        }
                    }
                    ui.label(file.byte_len.to_string());
                    match String::from_utf16(&file.description) {
                        Ok(description) => {
                            ui.label(description.trim_end_matches(' '));
                        }
                        Err(error) => {
                            ui.colored_label(
                                egui::Color32::LIGHT_RED,
                                format!("Invalid UTF-16 description: {error}"),
                            );
                        }
                    }
                    ui.end_row();
                }
            });
        action.list_focused = *list_focused || (was_list_focused && list_navigation_key);
        action.keyboard_activation_owned |= action.list_focused;
        if action.list_focused {
            let movement = match (up, down) {
                (true, false) => Some(-1_isize),
                (false, true) => Some(1),
                _ => None,
            };
            if let Some(movement) = movement {
                let current = directory
                    .iter()
                    .position(|file| Some(file.number) == *selected_file_number);
                let next = match current {
                    Some(index) if movement < 0 => index.saturating_sub(1),
                    Some(index) => (index + 1).min(directory.len() - 1),
                    None if movement < 0 => directory.len() - 1,
                    None => 0,
                };
                *selected_file_number = Some(directory[next].number);
            }
            if let Some(index) = directory
                .iter()
                .position(|file| Some(file.number) == *selected_file_number)
            {
                row_responses[index].request_focus();
            }
            if enter {
                action.launch_file_number = *selected_file_number;
            }
        }
    });
    action
}
