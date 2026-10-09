use crate::{
    controls::{button, slider, uv},
    egui,
    state::EguiUiRuntime,
};
use pentimento_ipc::{
    UiToBevy, UvLayerBlendMode, UvLayerCommand, UvLayerPaintTarget, UvLayerState,
};

pub(crate) fn show(
    ui: &mut egui::Ui,
    state: &UvLayerState,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    ui.separator();
    ui.heading("UV layers");
    ui.add_enabled_ui(!state.active && !state.projection_preview, |ui| {
        egui::ComboBox::from_id_salt("uv_receiver")
            .selected_text(
                state
                    .receivers
                    .iter()
                    .find(|r| Some(r.mesh_id) == state.receiver)
                    .map_or("Choose receiver", |r| r.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for receiver in &state.receivers {
                    if ui
                        .selectable_label(Some(receiver.mesh_id) == state.receiver, &receiver.name)
                        .clicked()
                    {
                        uv(
                            commands,
                            UvLayerCommand::SelectReceiver {
                                mesh_id: receiver.mesh_id,
                            },
                        );
                    }
                }
            });
    });
    if let Some(notice) = &state.notice {
        ui.label(notice);
    }
    let ready = !state.active && !state.conflicted && !state.projection_preview;
    if !state.enabled {
        ui.small("Enabling keeps existing pixels but interprets them with linear compositing; legacy appearance may change. Existing projections become independent snapshots.");
        if button(ui, ready && state.receiver.is_some(), "Enable UV layers") {
            uv(commands, UvLayerCommand::Enable);
        }
        return;
    }
    ui.add_enabled_ui(ready, |ui| {
        ui.horizontal_wrapped(|ui| {
            if button(ui, state.can_undo, "Undo UV layer edit") {
                uv(commands, UvLayerCommand::Undo);
            }
            if button(ui, state.can_redo, "Redo UV layer edit") {
                uv(commands, UvLayerCommand::Redo);
            }
        });
        for (index, layer) in state.layers.iter().enumerate() {
            ui.push_id(("uv", layer.id), |ui| {
                if ui.selectable_label(layer.is_active, &layer.name).clicked() {
                    uv(commands, UvLayerCommand::Select { layer_id: layer.id });
                }
                ui.horizontal(|ui| {
                    let mut visible = layer.visible;
                    if ui.checkbox(&mut visible, "Visible").changed() {
                        uv(
                            commands,
                            UvLayerCommand::Visible {
                                layer_id: layer.id,
                                visible,
                            },
                        );
                    }
                    let mut locked = layer.locked;
                    if ui.checkbox(&mut locked, "Lock paint").changed() {
                        uv(
                            commands,
                            UvLayerCommand::Lock {
                                layer_id: layer.id,
                                locked,
                            },
                        );
                    }
                });
                if let Some(opacity) =
                    slider(ui, layer.opacity * 100.0, 0.0..=100.0, "UV opacity %")
                {
                    uv(
                        commands,
                        UvLayerCommand::Opacity {
                            layer_id: layer.id,
                            opacity: opacity / 100.0,
                        },
                    );
                }
                egui::ComboBox::from_id_salt("blend")
                    .selected_text(format!("{:?}", layer.blend_mode))
                    .show_ui(ui, |ui| {
                        for mode in [
                            UvLayerBlendMode::Normal,
                            UvLayerBlendMode::Multiply,
                            UvLayerBlendMode::Screen,
                            UvLayerBlendMode::Overlay,
                        ] {
                            if ui
                                .selectable_label(mode == layer.blend_mode, format!("{mode:?}"))
                                .clicked()
                            {
                                uv(
                                    commands,
                                    UvLayerCommand::BlendMode {
                                        layer_id: layer.id,
                                        mode,
                                    },
                                );
                            }
                        }
                    });
                let name = runtime
                    .uv_layer_names
                    .entry(layer.id)
                    .or_insert_with(|| (layer.name.clone(), layer.name.clone()));
                if name.0 != layer.name {
                    *name = (layer.name.clone(), layer.name.clone());
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut name.1)
                            .desired_width((ui.available_width() - 120.0).max(80.0)),
                    );
                    if button(ui, !name.1.trim().is_empty(), "Rename UV layer") {
                        uv(
                            commands,
                            UvLayerCommand::Rename {
                                layer_id: layer.id,
                                name: name.1.trim().into(),
                            },
                        );
                        name.1 = layer.name.clone();
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if button(ui, index > 0, "Move UV up") {
                        uv(
                            commands,
                            UvLayerCommand::Reorder {
                                layer_id: layer.id,
                                new_index: state.layers.len() - index,
                            },
                        );
                    }
                    if button(ui, index + 1 < state.layers.len(), "Move UV down") {
                        uv(
                            commands,
                            UvLayerCommand::Reorder {
                                layer_id: layer.id,
                                new_index: state.layers.len() - index - 2,
                            },
                        );
                    }
                    if ui.button("Duplicate UV layer").clicked() {
                        uv(commands, UvLayerCommand::Duplicate { layer_id: layer.id });
                    }
                    if button(ui, state.layers.len() > 1, "Delete UV layer") {
                        uv(commands, UvLayerCommand::Delete { layer_id: layer.id });
                    }
                });
                if layer.has_mask {
                    let mut enabled = layer.mask_enabled;
                    if ui.checkbox(&mut enabled, "Enable mask").changed() {
                        uv(
                            commands,
                            UvLayerCommand::MaskEnabled {
                                layer_id: layer.id,
                                enabled,
                            },
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                layer.paint_target == UvLayerPaintTarget::Color,
                                "Paint color",
                            )
                            .clicked()
                        {
                            uv(
                                commands,
                                UvLayerCommand::PaintTarget {
                                    layer_id: layer.id,
                                    target: UvLayerPaintTarget::Color,
                                },
                            );
                        }
                        if ui
                            .add_enabled(
                                layer.mask_enabled,
                                egui::Button::selectable(
                                    layer.paint_target == UvLayerPaintTarget::Mask,
                                    "Paint mask",
                                ),
                            )
                            .clicked()
                        {
                            uv(
                                commands,
                                UvLayerCommand::PaintTarget {
                                    layer_id: layer.id,
                                    target: UvLayerPaintTarget::Mask,
                                },
                            );
                        }
                        if ui.button("Remove mask").clicked() {
                            uv(commands, UvLayerCommand::RemoveMask { layer_id: layer.id });
                        }
                    });
                } else if ui.button("Add mask").clicked() {
                    uv(commands, UvLayerCommand::AddMask { layer_id: layer.id });
                }
                ui.separator();
            });
        }
        ui.text_edit_singleline(&mut runtime.new_uv_layer_name);
        if ui.button("Create UV layer").clicked() {
            uv(
                commands,
                UvLayerCommand::Create {
                    name: runtime.new_uv_layer_name.trim().into(),
                },
            );
        }
    });
    ui.small("White mask reveals; black conceals. UV edits share one bounded Undo/Redo history. External edits require receiver reconciliation.");
    if state.projection_preview {
        ui.label("Live preview owns this receiver, layer and paint target. Apply or Cancel preview before changing the stack.");
    }
    if state.conflicted {
        ui.label("UV ownership changed. Reopen the owned project before editing this receiver.");
    }
}
