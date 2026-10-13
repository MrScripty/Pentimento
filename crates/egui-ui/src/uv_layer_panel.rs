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
    ui.strong("UV layers");
    ui.add_enabled_ui(!state.active && !state.projection_preview, |ui| {
        egui::ComboBox::from_id_salt("uv_receiver")
            .selected_text(
                state
                    .receivers
                    .iter()
                    .find(|r| Some(r.mesh_id) == state.receiver)
                    .map_or("Choose receiver", |r| r.name.as_str()),
            )
            .width(ui.available_width().min(270.0))
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
        ui.small("Enable to edit independent layers. Existing pixels are preserved and use linear compositing; their appearance may change.");
        if button(ui, ready && state.receiver.is_some(), "Enable UV layers") {
            uv(commands, UvLayerCommand::Enable);
        }
        return;
    }
    ui.add_enabled_ui(ready, |ui| {
        // Receiver snapshots already arrive top-to-bottom; commands use the
        // owner's bottom-to-top indices. Never reorder the accepted snapshot.
        egui::ScrollArea::vertical()
            .id_salt("uv_stack")
            .max_height(140.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for layer in &state.layers {
                    ui.push_id(("uv_row", layer.id), |ui| {
                        ui.horizontal(|ui| {
                            let mut visible = layer.visible;
                            if ui
                                .checkbox(&mut visible, "")
                                .on_hover_text("UV layer visibility")
                                .changed()
                            {
                                uv(
                                    commands,
                                    UvLayerCommand::Visible {
                                        layer_id: layer.id,
                                        visible,
                                    },
                                );
                            }
                            if ui
                                .add_sized(
                                    [ui.available_width(), 26.0],
                                    egui::Button::selectable(layer.is_active, &layer.name)
                                        .truncate(),
                                )
                                .on_hover_text(format!(
                                    "{} · {:.0}%{}{}",
                                    layer.name,
                                    layer.opacity * 100.0,
                                    if layer.locked { " · paint locked" } else { "" },
                                    if layer.has_mask { " · mask" } else { "" }
                                ))
                                .clicked()
                            {
                                uv(commands, UvLayerCommand::Select { layer_id: layer.id });
                            }
                        });
                    });
                }
            });
        if let Some((index, layer)) = state.layers.iter().enumerate().find(|(_, l)| l.is_active) {
            ui.push_id(("uv_properties", layer.id), |ui| {
                ui.horizontal_wrapped(|ui| {
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
                            .desired_width((ui.available_width() - 120.0).max(40.0)),
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
                    ui.horizontal_wrapped(|ui| {
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
            });
        } else {
            ui.small("Select a layer to edit its properties.");
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut runtime.new_uv_layer_name)
                    .hint_text("New UV layer name")
                    .desired_width((ui.available_width() - 115.0).max(40.0)),
            );
            if ui.button("Create UV layer").clicked() {
                uv(
                    commands,
                    UvLayerCommand::Create {
                        name: runtime.new_uv_layer_name.trim().into(),
                    },
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            if button(ui, state.can_undo, "Undo UV layer edit") {
                uv(commands, UvLayerCommand::Undo);
            }
            if button(ui, state.can_redo, "Redo UV layer edit") {
                uv(commands, UvLayerCommand::Redo);
            }
        });
    });
    ui.small("White masks reveal; black conceals. All UV edits share one Undo/Redo history.");
    if state.projection_preview {
        ui.label("Apply or Cancel the live preview before editing this stack.");
    }
    if state.conflicted {
        ui.label("UV ownership changed. Reopen the owned project before editing this receiver.");
    }
}
