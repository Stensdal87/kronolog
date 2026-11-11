//! Vinduet "Genvejstaster", hvor brugeren ændrer genvejene.

use super::*;
use crate::shortcuts::{Action, KeyCombo, Recorded, record_key};

#[derive(Default)]
pub(super) struct ShortcutsDialog {
    /// Den genvej der optages: handling og om det er den globale.
    recording: Option<(Action, bool)>,
    message: Option<String>,
}

impl KronoLogApp {
    pub(super) fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.shortcuts_dialog.take() else { return };
        let globals_supported = self.hotkeys.is_some();

        if let Some((action, global)) = dialog.recording
            && let Some(recorded) = record_key(ctx)
        {
            dialog.recording = None;
            match recorded {
                Recorded::Cancel => {}
                Recorded::Clear => self.set_shortcut(action, global, None),
                Recorded::Combo(combo) => match self.check_shortcut(action, global, combo) {
                    Ok(()) => {
                        self.set_shortcut(action, global, Some(combo));
                        dialog.message = None;
                    }
                    Err(e) => dialog.message = Some(e),
                },
            }
            self.refresh_global_hotkeys(dialog.recording.is_some());
        }

        let mut close = false;
        let mut start_recording = None;
        let modal = egui::Modal::new(egui::Id::unique("shortcuts_dialog")).show(ctx, |ui| {
            // Skal kunne være i programmets smalle standardvindue (320 px).
            ui.set_width((ctx.content_rect().width() - 40.0).clamp(260.0, 360.0));
            ui.heading(tr!("shortcuts.title"));
            ui.label(
                RichText::new(tr!("shortcuts.help"))
                    .weak()
                    .small(),
            );
            ui.add_space(6.0);

            egui::Grid::new("shortcuts").num_columns(3).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
                ui.label(RichText::new(tr!("shortcuts.col_action")).strong());
                ui.label(RichText::new(tr!("shortcuts.col_local")).strong());
                ui.label(RichText::new(tr!("shortcuts.col_global")).strong())
                    .on_hover_text(tr!("shortcuts.col_global_hint"));
                ui.end_row();

                for action in Action::ALL {
                    ui.label(action.label());
                    let local = self.data.settings.shortcuts.local(action);
                    if shortcut_cell(ui, local, dialog.recording == Some((action, false))).clicked() {
                        start_recording = Some((action, false));
                    }
                    if !action.can_be_global() {
                        ui.label("");
                    } else if !globals_supported {
                        ui.label(RichText::new("Windows").weak().small())
                            .on_hover_text(tr!("shortcuts.linux_later"));
                    } else {
                        let global = self.data.settings.shortcuts.global(action);
                        ui.horizontal(|ui| {
                            if shortcut_cell(ui, global, dialog.recording == Some((action, true))).clicked() {
                                start_recording = Some((action, true));
                            }
                            if let Some(error) = self.hotkeys.as_ref().and_then(|h| h.error(action)) {
                                ui.colored_label(ui.visuals().warn_fg_color, "⚠").on_hover_text(error);
                            }
                        });
                    }
                    ui.end_row();
                }
            });

            if let Some(message) = &dialog.message {
                ui.add_space(4.0);
                ui.colored_label(ui.visuals().warn_fg_color, message);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("shortcuts.reset")).clicked() {
                    self.data.settings.shortcuts.reset();
                    self.save();
                    self.refresh_global_hotkeys(false);
                    dialog.message = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= ui.button(tr!("common.close")).clicked();
                });
            });
        });

        if let Some(target) = start_recording {
            dialog.recording = Some(target);
            dialog.message = None;
            // Ellers fanger Windows en eksisterende global genvej, før vi ser tasterne.
            self.refresh_global_hotkeys(true);
        }
        // Esc under optagelse er allerede brugt til at fortryde.
        if dialog.recording.is_none() && modal.should_close() {
            close = true;
        }
        if close {
            self.refresh_global_hotkeys(false);
        } else {
            self.shortcuts_dialog = Some(dialog);
        }
    }

    /// Tjekker at en ny genvej kan bruges.
    fn check_shortcut(&self, action: Action, global: bool, combo: KeyCombo) -> Result<(), String> {
        if !combo.has_modifier() && !combo.is_function_key() {
            return Err(tr!("shortcuts.err_modifier", combo = combo));
        }
        let shortcuts = &self.data.settings.shortcuts;
        let conflict = if global {
            shortcuts.global_conflict(combo, action)
        } else {
            shortcuts.local_conflict(combo, action)
        };
        match conflict {
            Some(other) => Err(tr!("shortcuts.err_taken", combo = combo, action = other.label())),
            None => Ok(()),
        }
    }

    fn set_shortcut(&mut self, action: Action, global: bool, combo: Option<KeyCombo>) {
        let shortcuts = &mut self.data.settings.shortcuts;
        if global {
            shortcuts.set_global(action, combo);
        } else {
            shortcuts.set_local(action, combo);
        }
        self.save();
    }

    /// Registrerer de globale genveje igen. `paused` frigiver dem midlertidigt,
    /// mens der optages en ny genvej.
    pub(super) fn refresh_global_hotkeys(&self, paused: bool) {
        if let Some(hotkeys) = &self.hotkeys {
            hotkeys.set_bindings(if paused { Vec::new() } else { self.data.settings.shortcuts.globals() });
        }
    }
}

/// En knap der viser genvejen, eller "Tryk taster…" mens den optages.
fn shortcut_cell(ui: &mut egui::Ui, combo: Option<KeyCombo>, recording: bool) -> egui::Response {
    let text = if recording {
        RichText::new(tr!("shortcuts.press_keys")).small().color(ui.visuals().warn_fg_color)
    } else {
        match combo {
            Some(combo) => RichText::new(combo.to_string()).small(),
            None => RichText::new("–").weak(),
        }
    };
    let button = if recording { egui::Button::selectable(true, text) } else { egui::Button::new(text) };
    ui.add(button.min_size(Vec2::new(78.0, 0.0)))
}
