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
    let (catalog, selected, name) = if sculpt_mode {
        (
            &saved.sculpt,
            saved.selected_sculpt,
            &mut runtime.sculpt_preset_name,
        )
    } else {
        (
            &saved.paint,
            saved.selected_paint,
            &mut runtime.paint_preset_name,
        )
    };
    ui.collapsing("Saved brushes", |ui| {
        ui.add_enabled_ui(saved.available && !saved.active, |ui| {
            egui::ComboBox::from_id_salt(if sculpt_mode {
                "saved_sculpt"
            } else {
                "saved_paint"
            })
            .selected_text(
                catalog
                    .iter()
                    .find(|p| Some(p.id) == selected)
                    .map_or("Choose saved brush", |p| p.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for preset in catalog {
                    if ui
                        .selectable_label(Some(preset.id) == selected, &preset.name)
                        .clicked()
                    {
                        if sculpt_mode {
                            sculpt(
                                commands,
                                SculptCommand::SelectSavedBrushPreset {
                                    preset_id: preset.id,
                                },
                            );
                        } else {
                            paint(
                                commands,
                                PaintCommand::SelectSavedBrushPreset {
                                    preset_id: preset.id,
                                },
                            );
                        }
                    }
                }
            });
            ui.text_edit_singleline(name);
            if button(ui, !name.trim().is_empty(), "Save current brush") {
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
        if let Some(notice) = &saved.notice {
            ui.label(notice);
        }
        ui.small("Saved locally. Using a saved brush waits for backend acknowledgement.");
    });
}
