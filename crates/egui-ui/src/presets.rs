use crate::{
    controls::{button, paint, sculpt},
    egui,
    state::{EguiUiRuntime, EguiUiSnapshot},
};
use pentimento_ipc::{PaintCommand, SculptCommand, UiToBevy};

pub(crate) fn show(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    sculpt_mode: bool,
    commands: &mut Vec<UiToBevy>,
) {
    let saved = &snapshot.saved_brushes;
    let (catalog, selected, name, choice) = if sculpt_mode {
        (
            &saved.sculpt,
            saved.selected_sculpt,
            &mut runtime.sculpt_preset_name,
            &mut runtime.sculpt_saved_choice,
        )
    } else {
        (
            &saved.paint,
            saved.selected_paint,
            &mut runtime.paint_preset_name,
            &mut runtime.paint_saved_choice,
        )
    };
    let choice = choice.get_or_insert((selected, selected));
    if saved.active || choice.0 != selected {
        *choice = (selected, selected);
    }
    if choice
        .1
        .is_some_and(|id| !catalog.iter().any(|preset| preset.id == id))
    {
        choice.1 = None;
    }
    ui.collapsing("Saved brushes", |ui| {
        let mode = if sculpt_mode { "sculpt" } else { "paint" };
        ui.add_enabled_ui(saved.available && !saved.active, |ui| {
            egui::ComboBox::from_id_salt(if sculpt_mode {
                "saved_sculpt"
            } else {
                "saved_paint"
            })
            .selected_text(
                catalog
                    .iter()
                    .find(|p| Some(p.id) == choice.1)
                    .map_or("Choose saved brush", |p| p.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for preset in catalog {
                    if ui
                        .selectable_label(Some(preset.id) == choice.1, &preset.name)
                        .clicked()
                    {
                        choice.1 = Some(preset.id);
                    }
                }
            });
            if button(ui, choice.1.is_some(), &format!("Use {mode} brush")) {
                if let Some(preset_id) = choice.1 {
                    if sculpt_mode {
                        sculpt(
                            commands,
                            SculptCommand::SelectSavedBrushPreset { preset_id },
                        );
                    } else {
                        paint(commands, PaintCommand::SelectSavedBrushPreset { preset_id });
                    }
                }
                // Display accepted selection again until the backend acknowledges recall.
                choice.1 = selected;
            }
            ui.add(
                egui::TextEdit::singleline(name)
                    .char_limit(64)
                    .hint_text(if sculpt_mode {
                        "Sculpt preset name"
                    } else {
                        "Paint preset name"
                    }),
            );
            if button(ui, !name.trim().is_empty(), &format!("Save {mode} brush")) {
                if sculpt_mode {
                    sculpt(
                        commands,
                        SculptCommand::SaveBrushPreset {
                            name: name.trim().into(),
                        },
                    );
                } else {
                    paint(
                        commands,
                        PaintCommand::SaveBrushPreset {
                            name: name.trim().into(),
                        },
                    );
                }
            }
        });
        if let Some(preset) = catalog.iter().find(|preset| Some(preset.id) == selected) {
            ui.small(format!("Current saved brush: {}", preset.name));
        }
        if let Some(notice) = &saved.notice {
            ui.label(notice);
        }
        ui.small(if sculpt_mode {
            "Saves tool, radius, strength, hardness, falloff and auto smoothing."
        } else {
            "Saves tip settings, pressure sizes, color and Brush / Eraser."
        });
        ui.small("Saved on this computer. Reusing a name replaces that brush. Up to 64 per mode.");
    });
}
