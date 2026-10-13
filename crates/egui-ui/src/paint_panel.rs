use crate::{
    controls::{button, paint, slider},
    egui, paint_color, presets,
    state::{EguiUiRuntime, EguiUiSnapshot},
    uv_layer_panel,
};
use pentimento_ipc::{
    BlendMode, ColorSampleSource, PaintCommand, PaintTarget, UiToBevy, UvLayerPaintTarget,
};

/// High-frequency controls stay above the viewport, outside scrolling dock content.
pub(crate) fn toolbar(ui: &mut egui::Ui, snapshot: &EguiUiSnapshot, commands: &mut Vec<UiToBevy>) {
    let Some(state) = &snapshot.paint else { return };
    let target = &state.target;
    let direct = target.mode == PaintTarget::DirectUv;
    let layered = target.uv_layers.as_ref().is_some_and(|s| s.enabled);
    let preview = target
        .uv_layers
        .as_ref()
        .is_some_and(|s| s.projection_preview);
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(!target.active, |ui| {
            if ui.selectable_label(!direct, "Canvas projection").clicked() {
                paint(
                    commands,
                    PaintCommand::SetTarget {
                        target: PaintTarget::Canvas,
                    },
                );
            }
            if ui
                .add_enabled(
                    !preview && target.direct_available,
                    egui::Button::selectable(direct, "DirectUV surface"),
                )
                .clicked()
            {
                paint(
                    commands,
                    PaintCommand::SetTarget {
                        target: PaintTarget::DirectUv,
                    },
                );
            }
        });
        ui.separator();
        ui.add_enabled_ui(!(direct && target.active), |ui| {
            for (mode, label) in [(BlendMode::Normal, "Brush"), (BlendMode::Erase, "Eraser")] {
                if ui
                    .selectable_label(state.settings.blend_mode == mode, label)
                    .clicked()
                {
                    paint(commands, PaintCommand::SetBlendMode { mode });
                }
            }
        });
        ui.separator();
        let (undo, redo) = match (direct, layered) {
            (false, _) => ("Undo canvas stroke", "Redo canvas stroke"),
            (true, false) => ("Undo surface stroke", "Redo surface stroke"),
            (true, true) => ("Undo UV edit", "Redo UV edit"),
        };
        if button(ui, state.can_undo && !target.active, undo) {
            paint(commands, PaintCommand::Undo);
        }
        if button(ui, state.can_redo && !target.active, redo) {
            paint(commands, PaintCommand::Redo);
        }
        if target.active && ui.button("Cancel current stroke").clicked() {
            paint(commands, PaintCommand::CancelStroke);
        }
    });
    ui.add_enabled_ui(!(direct && target.active), |ui| {
        ui.horizontal_wrapped(|ui| {
            let settings = &state.settings;
            if let Some(radius) = crate::controls::toolbar_value(
                ui,
                settings.size / 2.0,
                0.5..=256.0,
                if direct {
                    "Radius (atlas px)"
                } else {
                    "Radius (canvas px)"
                },
            ) {
                paint(commands, PaintCommand::SetBrushSize { size: radius * 2.0 });
            }
            if let Some(opacity) = crate::controls::toolbar_value(
                ui,
                settings.opacity * 100.0,
                0.0..=100.0,
                "Opacity %",
            ) {
                paint(
                    commands,
                    PaintCommand::SetBrushOpacity {
                        opacity: opacity / 100.0,
                    },
                );
            }
            if let Some(hardness) = crate::controls::toolbar_value(
                ui,
                settings.hardness * 100.0,
                0.0..=100.0,
                "Hardness %",
            ) {
                paint(
                    commands,
                    PaintCommand::SetBrushHardness {
                        hardness: hardness / 100.0,
                    },
                );
            }
            if let Some(spacing) = crate::controls::toolbar_value(
                ui,
                settings.spacing * 100.0,
                1.0..=100.0,
                "Dab spacing %",
            ) {
                paint(
                    commands,
                    PaintCommand::SetBrushSpacing {
                        spacing: spacing / 100.0,
                    },
                );
            }
        });
    });
}

pub(crate) fn shelf(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    if ui.available_width() >= 500.0 {
        ui.columns(2, |columns| {
            brush_tiles(&mut columns[0], snapshot, commands);
            presets::show(&mut columns[1], snapshot, runtime, false, commands);
        });
    } else {
        brush_tiles(ui, snapshot, commands);
        presets::show(ui, snapshot, runtime, false, commands);
    }
}

fn brush_tiles(ui: &mut egui::Ui, snapshot: &EguiUiSnapshot, commands: &mut Vec<UiToBevy>) {
    ui.strong("Brush presets");
    if let Some(state) = &snapshot.paint {
        ui.add_enabled_ui(
            !snapshot.saved_brushes.active
                && !(state.target.mode == PaintTarget::DirectUv && state.target.active),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    for preset in &state.presets {
                        if crate::controls::tile(
                            ui,
                            !state.settings.customized && state.settings.preset_id == preset.id,
                            &preset.name,
                        )
                        .clicked()
                        {
                            paint(
                                commands,
                                PaintCommand::SelectBrushPreset {
                                    preset_id: preset.id,
                                },
                            );
                        }
                    }
                });
            },
        );
        if state.settings.customized {
            ui.small("Custom round brush");
        }
    }
}

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
    let layered = target.uv_layers.as_ref().is_some_and(|s| s.enabled);
    let locked = target.active;
    let settings = &state.settings;
    ui.strong("Brush color & tip");
    paint_color::preview(ui, settings);
    let mask = target.uv_layers.as_ref().is_some_and(|s| {
        s.layers
            .iter()
            .any(|l| l.is_active && l.paint_target == UvLayerPaintTarget::Mask)
    });
    ui.add_enabled_ui(!(direct && locked) && settings.blend_mode != BlendMode::Erase, |ui| {
        if direct && mask {
            let gray = settings.color[0] * 0.2126 + settings.color[1] * 0.7152 + settings.color[2] * 0.0722;
            if let Some(gray) = slider(ui, gray * 100.0, 0.0..=100.0, "Mask grayscale %") {
                let g = gray / 100.0;
                paint(commands, PaintCommand::SetBrushColor { color: [g, g, g, 1.0] });
            }
            ui.small("Black conceals; white reveals. Eraser reveals white.");
        } else {
            paint_color::edit(ui, settings, runtime, !(direct && locked) && settings.blend_mode != BlendMode::Erase, commands);
            if mask { ui.small("Canvas brightness supplies mask coverage; transparency leaves it unchanged."); }
        }
    });
    if !direct {
        ui.add_enabled_ui(!snapshot.color_sampling.active, |ui| {
            ui.horizontal_wrapped(|ui| {
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
                    .selected_text(match snapshot.color_sampling.source {
                        ColorSampleSource::VisibleLayers => "Visible layers",
                        ColorSampleSource::ActiveLayer => "Active layer",
                    })
                    .show_ui(ui, |ui| {
                        for (source, label) in [
                            (ColorSampleSource::VisibleLayers, "Visible layers"),
                            (ColorSampleSource::ActiveLayer, "Active layer"),
                        ] {
                            if ui
                                .selectable_label(snapshot.color_sampling.source == source, label)
                                .clicked()
                            {
                                paint(commands, PaintCommand::SetColorSampleSource { source });
                            }
                        }
                    });
            });
        });
        canvas_layers(ui, snapshot, runtime, locked, commands);
        ui.separator();
        ui.strong("Projection");
        ui.horizontal_wrapped(|ui| {
            let mut live = snapshot.live_projection;
            if ui
                .add_enabled(
                    !locked && !conflicted,
                    egui::Checkbox::new(
                        &mut live,
                        if layered {
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
        });
        if button(
            ui,
            !locked && !conflicted,
            if layered {
                "Apply canvas to active UV layer"
            } else {
                "Apply canvas to UV surfaces"
            },
        ) {
            paint(commands, PaintCommand::ProjectToScene);
        }
        if layered {
            if preview && ui.button("Cancel UV preview").clicked() {
                paint(commands, PaintCommand::CancelUvProjection);
            }
            ui.small("Apply commits once and pauses live. Apply or Cancel before editing UV layers or saving.");
        }
    }
    if let Some(layers) = &target.uv_layers {
        uv_layer_panel::show(ui, layers, runtime, commands);
    }
    if let Some(notice) = &target.notice {
        ui.label(notice);
    }
    if direct || layered {
        ui.small(format!(
            "UV history: {:.1} / {:.1} MiB retained · {:.1} MiB pending · {} expired",
            target.retained_bytes as f64 / 1048576.0,
            target.limit_bytes as f64 / 1048576.0,
            target.pending_bytes as f64 / 1048576.0,
            target.evicted_strokes
        ));
    }
    ui.separator();
    ui.small(if direct { "Paint the visible UV surface. Escape cancels a stroke. Source canvas and live projection are paused." } else { "Canvas Undo affects the source. UV Undo affects receiver layers. Shift+Tab leaves Paint; Tab leaves / returns to canvas view." });
    ui.small("Middle-drag: orbit · Shift+middle: pan · Scroll: zoom");
}

fn canvas_layers(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    locked: bool,
    commands: &mut Vec<UiToBevy>,
) {
    ui.separator();
    ui.strong("Canvas layers");
    ui.add_enabled_ui(!locked, |ui| {
        // The snapshot is bottom-to-top; present the topmost layer first.
        egui::ScrollArea::vertical()
            .id_salt("canvas_stack")
            .max_height(140.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for layer in snapshot.layers.iter().rev() {
                    ui.push_id(("canvas_row", layer.id), |ui| {
                        ui.horizontal(|ui| {
                            let mut visible = layer.visible;
                            if ui
                                .checkbox(&mut visible, "")
                                .on_hover_text("Layer visibility")
                                .changed()
                            {
                                paint(
                                    commands,
                                    PaintCommand::SetLayerVisibility {
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
                                .clicked()
                            {
                                paint(
                                    commands,
                                    PaintCommand::SetActiveLayer { layer_id: layer.id },
                                );
                            }
                        });
                    });
                }
            });
        if let Some((index, layer)) = snapshot
            .layers
            .iter()
            .enumerate()
            .find(|(_, l)| l.is_active)
        {
            ui.push_id(("canvas_properties", layer.id), |ui| {
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
                            .desired_width((ui.available_width() - 65.0).max(40.0)),
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
                ui.horizontal_wrapped(|ui| {
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
            });
        } else {
            ui.small("Select a layer to edit its properties.");
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut runtime.new_canvas_layer_name)
                    .hint_text("New layer name")
                    .desired_width((ui.available_width() - 120.0).max(40.0)),
            );
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
