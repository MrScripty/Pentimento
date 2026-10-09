use crate::{
    controls::{button, paint, slider},
    egui, presets,
    state::{EguiUiRuntime, EguiUiSnapshot},
    uv_layer_panel,
};
use pentimento_ipc::{
    BlendMode, ColorSampleSource, PaintCommand, PaintTarget, UiToBevy, UvLayerPaintTarget,
};

pub(crate) fn show(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    let Some(state) = &snapshot.paint else {
        ui.label("Waiting for paint settings…");
        return;
    };
    let target = &state.target;
    let direct = target.mode == PaintTarget::DirectUv;
    let preview = target
        .uv_layers
        .as_ref()
        .is_some_and(|s| s.projection_preview);
    let conflicted = target
        .uv_layers
        .as_ref()
        .is_some_and(|s| s.enabled && s.conflicted);
    let locked = target.active;
    ui.heading(if direct {
        "DirectUV surface"
    } else {
        "Canvas projection"
    });
    ui.horizontal_wrapped(|ui| {
        if button(ui, !locked, "Canvas projection") {
            paint(
                commands,
                PaintCommand::SetTarget {
                    target: PaintTarget::Canvas,
                },
            );
        }
        if button(
            ui,
            !locked && !preview && target.direct_available,
            "DirectUV surface",
        ) {
            paint(
                commands,
                PaintCommand::SetTarget {
                    target: PaintTarget::DirectUv,
                },
            );
        }
    });
    if let Some(notice) = &target.notice {
        ui.label(notice);
    }
    let settings = &state.settings;
    ui.add_enabled_ui(!locked, |ui| {
        ui.horizontal(|ui| {
            if ui.selectable_label(settings.blend_mode==BlendMode::Normal,"Brush").clicked() {paint(commands,PaintCommand::SetBlendMode{mode:BlendMode::Normal});}
            if ui.selectable_label(settings.blend_mode==BlendMode::Erase,"Eraser").clicked() {paint(commands,PaintCommand::SetBlendMode{mode:BlendMode::Erase});}
        });
        egui::ComboBox::from_id_salt("paint_preset").selected_text(if settings.customized {"Custom round brush"} else {state.presets.iter().find(|p|p.id==settings.preset_id).map_or("Round brush",|p|p.name.as_str())}).show_ui(ui,|ui| {
            for preset in &state.presets {
                if ui.selectable_label(!settings.customized && settings.preset_id==preset.id,&preset.name).clicked() {paint(commands,PaintCommand::SelectBrushPreset{preset_id:preset.id});}
            }
        });
        if let Some(radius)=slider(ui,settings.size/2.0,0.5..=256.0,if direct {"Radius (atlas px)"} else {"Radius (canvas px)"}) {paint(commands,PaintCommand::SetBrushSize{size:radius*2.0});}
        if let Some(opacity)=slider(ui,settings.opacity*100.0,0.0..=100.0,"Opacity %") {paint(commands,PaintCommand::SetBrushOpacity{opacity:opacity/100.0});}
        if let Some(hardness)=slider(ui,settings.hardness*100.0,0.0..=100.0,"Hardness %") {paint(commands,PaintCommand::SetBrushHardness{hardness:hardness/100.0});}
        if let Some(spacing)=slider(ui,settings.spacing*100.0,1.0..=100.0,"Dab spacing %") {paint(commands,PaintCommand::SetBrushSpacing{spacing:spacing/100.0});}
        let mask=target.uv_layers.as_ref().is_some_and(|s|s.layers.iter().any(|l|l.is_active && l.paint_target==UvLayerPaintTarget::Mask));
        ui.add_enabled_ui(settings.blend_mode!=BlendMode::Erase,|ui| {
            if direct && mask {
                let gray=settings.color[0]*0.2126+settings.color[1]*0.7152+settings.color[2]*0.0722;
                if let Some(gray)=slider(ui,gray*100.0,0.0..=100.0,"Mask grayscale %") {let g=gray/100.0;paint(commands,PaintCommand::SetBrushColor{color:[g,g,g,1.0]});}
                ui.small("Black conceals; white reveals. Eraser reveals white.");
            } else {
                // This egui API accepts linear RGB and handles display conversion itself.
                let mut rgb=[settings.color[0],settings.color[1],settings.color[2]];
                ui.horizontal(|ui| {ui.label("Color");if ui.color_edit_button_rgb(&mut rgb).changed() {paint(commands,PaintCommand::SetBrushColor{color:[rgb[0],rgb[1],rgb[2],1.0]});}});
                if mask {ui.small("Canvas linear brightness supplies mask coverage. Transparent source pixels leave it unchanged.");}
            }
        });
    });
    presets::show(ui, snapshot, runtime, false, commands);
    if !direct {
        ui.add_enabled_ui(!snapshot.color_sampling.active, |ui| {
            if ui
                .button(if snapshot.color_sampling.enabled {
                    "Cancel color sampling"
                } else {
                    "Sample canvas color"
                })
                .clicked()
            {
                paint(
                    commands,
                    PaintCommand::SetColorSampling {
                        enabled: !snapshot.color_sampling.enabled,
                    },
                );
            }
            egui::ComboBox::from_id_salt("sample_source")
                .selected_text(format!("{:?}", snapshot.color_sampling.source))
                .show_ui(ui, |ui| {
                    for source in [
                        ColorSampleSource::VisibleLayers,
                        ColorSampleSource::ActiveLayer,
                    ] {
                        if ui
                            .selectable_label(
                                snapshot.color_sampling.source == source,
                                format!("{source:?}"),
                            )
                            .clicked()
                        {
                            paint(commands, PaintCommand::SetColorSampleSource { source });
                        }
                    }
                });
        });
    }
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        if button(ui, state.can_undo && !locked, "Undo paint stroke") {
            paint(commands, PaintCommand::Undo);
        }
        if button(ui, state.can_redo && !locked, "Redo paint stroke") {
            paint(commands, PaintCommand::Redo);
        }
        if button(ui, locked, "Cancel current stroke") {
            paint(commands, PaintCommand::CancelStroke);
        }
    });
    ui.small(format!(
        "UV history payload: {:.1} / {:.1} MiB retained; {:.1} MiB pending; {} older edits expired.",
        target.retained_bytes as f64 / 1048576.0,
        target.limit_bytes as f64 / 1048576.0,
        target.pending_bytes as f64 / 1048576.0,
        target.evicted_strokes
    ));
    if !direct {
        let mut live = snapshot.live_projection;
        if ui
            .add_enabled(
                !locked && !conflicted,
                egui::Checkbox::new(
                    &mut live,
                    if target.uv_layers.as_ref().is_some_and(|s| s.enabled) {
                        "Live UV preview"
                    } else {
                        "Live projection"
                    },
                ),
            )
            .changed()
        {
            paint(commands, PaintCommand::SetLiveProjection { enabled: live });
        }
        let mut source = state.source_visible;
        if ui.checkbox(&mut source, "Show source canvas").changed() {
            paint(commands, PaintCommand::SetSourceVisible { visible: source });
        }
        if button(ui, !locked && !conflicted, "Apply canvas to UV surfaces") {
            paint(commands, PaintCommand::ProjectToScene);
        }
        if target.uv_layers.as_ref().is_some_and(|s| s.enabled) {
            if button(ui, preview && !locked, "Cancel UV preview") {
                paint(commands, PaintCommand::CancelUvProjection);
            }
            ui.small("Apply commits once and pauses live. Cancel retains Canvas edits and UV history. Apply or Cancel before DirectUV, layer edits or Save.");
        }
        canvas_layers(ui, snapshot, runtime, locked, commands);
    }
    if let Some(layers) = &target.uv_layers {
        uv_layer_panel::show(ui, layers, runtime, commands);
    }
}
fn canvas_layers(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    locked: bool,
    commands: &mut Vec<UiToBevy>,
) {
    ui.collapsing("Canvas layers", |ui| {
        ui.add_enabled_ui(!locked, |ui| {
            for (index, layer) in snapshot.layers.iter().enumerate().rev() {
                ui.push_id(layer.id, |ui| {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(layer.is_active, &layer.name).clicked() {
                            paint(
                                commands,
                                PaintCommand::SetActiveLayer { layer_id: layer.id },
                            );
                        }
                        let mut visible = layer.visible;
                        if ui.checkbox(&mut visible, "Visible").changed() {
                            paint(
                                commands,
                                PaintCommand::SetLayerVisibility {
                                    layer_id: layer.id,
                                    visible,
                                },
                            );
                        }
                    });
                    if let Some(opacity) =
                        slider(ui, layer.opacity * 100.0, 0.0..=100.0, "Layer opacity %")
                    {
                        paint(
                            commands,
                            PaintCommand::SetLayerOpacity {
                                layer_id: layer.id,
                                opacity: opacity / 100.0,
                            },
                        );
                    }
                    ui.horizontal(|ui| {
                        if button(ui, index + 1 < snapshot.layers.len(), "Up") {
                            paint(
                                commands,
                                PaintCommand::ReorderLayer {
                                    layer_id: layer.id,
                                    new_index: (index + 1) as u32,
                                },
                            );
                        }
                        if button(ui, index > 0, "Down") {
                            paint(
                                commands,
                                PaintCommand::ReorderLayer {
                                    layer_id: layer.id,
                                    new_index: (index - 1) as u32,
                                },
                            );
                        }
                        if button(ui, snapshot.layers.len() > 1, "Delete") {
                            paint(commands, PaintCommand::RemoveLayer { layer_id: layer.id });
                        }
                    });
                    let name = runtime
                        .canvas_layer_names
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
                        if button(ui, !name.1.trim().is_empty(), "Rename") {
                            paint(
                                commands,
                                PaintCommand::RenameLayer {
                                    layer_id: layer.id,
                                    name: name.1.trim().into(),
                                },
                            );
                            name.1 = layer.name.clone();
                        }
                    });
                });
            }
            ui.text_edit_singleline(&mut runtime.new_canvas_layer_name);
            if ui.button("Add canvas layer").clicked() {
                paint(
                    commands,
                    PaintCommand::AddLayer {
                        name: runtime.new_canvas_layer_name.trim().into(),
                    },
                );
            }
        });
    });
}
