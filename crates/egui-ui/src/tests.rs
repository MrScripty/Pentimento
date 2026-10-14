use crate::{
    egui,
    project_dialog::{Operation, ProjectDialog},
    *,
};
use pentimento_ipc::*;

struct Harness {
    ctx: egui::Context,
    snapshot: EguiUiSnapshot,
    runtime: EguiUiRuntime,
    time: f64,
    size: egui::Vec2,
    textures: crate::test_render::Textures,
}
impl Harness {
    fn replace_hex(&mut self, text: &str) -> Vec<UiToBevy> {
        let current = self.runtime.paint_hex_color.as_ref().unwrap().1.clone();
        assert!(self.click(&current).is_empty());
        assert!(self.ctx.egui_wants_keyboard_input());
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        self.frame(vec![
            key(
                egui::Key::A,
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            ),
            egui::Event::Text(text.into()),
            key(egui::Key::Enter, Default::default()),
        ])
        .1
    }
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|s| s.animation_time = 0.0);
        Self {
            ctx,
            snapshot: Default::default(),
            runtime: Default::default(),
            time: 0.0,
            size: egui::vec2(1200.0, 1800.0),
            textures: Default::default(),
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> (egui::FullOutput, Vec<UiToBevy>) {
        self.time += 0.05;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut commands = Vec::new();
        let mut output = self.ctx.run_ui(input, |ui| {
            commands = show_root_ui(ui.ctx(), &mut self.snapshot, &mut self.runtime)
        });
        // Preserve real atlas deltas for optional CPU capture, then acknowledge them.
        self.textures.apply(&mut output.textures_delta);
        (output, commands)
    }
    fn click(&mut self, label: &str) -> Vec<UiToBevy> {
        self.frame(vec![]);
        let (output, _) = self.frame(vec![]);
        fn locate(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| locate(s, label)),
                _ => None,
            }
        }
        let position = output
            .shapes
            .iter()
            .find_map(|s| locate(&s.shape, label).filter(|p| s.clip_rect.contains(*p)))
            .unwrap_or_else(|| panic!("No widget text: {label}"));
        self.frame(vec![egui::Event::PointerMoved(position)]);
        self.frame(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        self.frame(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }])
        .1
    }
    fn paint(&mut self) {
        self.snapshot.edit_mode = EditMode::Paint;
        apply_bevy_message(
            &mut self.snapshot,
            BevyToUi::PaintBrushStateChanged {
                settings: PaintBrushSettings {
                    preset_id: 7,
                    customized: false,
                    color: [0.2, 0.4, 0.6, 1.0],
                    size: 20.0,
                    opacity: 0.75,
                    hardness: 0.8,
                    spacing: 0.1,
                    blend_mode: BlendMode::Normal,
                },
                presets: vec![PaintBrushPresetInfo {
                    id: 7,
                    name: "Round".into(),
                }],
                can_undo: true,
                can_redo: true,
                source_visible: true,
                target: PaintTargetState {
                    direct_available: true,
                    limit_bytes: 64 * 1024 * 1024,
                    ..Default::default()
                },
            },
        );
    }
    fn sculpt(&mut self) {
        self.snapshot.edit_mode = EditMode::Sculpt;
        apply_bevy_message(
            &mut self.snapshot,
            BevyToUi::SculptBrushStateChanged {
                settings: Some(SculptBrushSettings {
                    tool: SculptTool::Push,
                    radius: 1.0,
                    strength: 0.3,
                    hardness: 0.7,
                    autosmooth: 0.5,
                    falloff: SculptFalloff::Smooth,
                }),
            },
        );
        apply_bevy_message(
            &mut self.snapshot,
            BevyToUi::SculptHistoryChanged {
                undo_strokes: 3,
                redo_strokes: 1,
                active: false,
                notice: Some("Older strokes expired within the history payload limit.".into()),
            },
        );
    }
    fn project(&mut self) {
        apply_bevy_message(
            &mut self.snapshot,
            BevyToUi::ProjectStateChanged {
                generation: "18446744073709551615".into(),
                path: None,
                available: true,
                active: false,
                blocked: false,
                notice: None,
            },
        );
    }
}

#[test]
fn actual_hex_field_commits_linear_rgb_and_rejects_invalid_input_without_optimism() {
    let mut h = Harness::new();
    h.paint();
    h.frame(vec![]);
    let original = h.snapshot.paint.as_ref().unwrap().settings.color;
    let commands = h.replace_hex("#808080");
    let color = commands
        .iter()
        .find_map(|c| match c {
            UiToBevy::PaintCommand(PaintCommand::SetBrushColor { color }) => Some(*color),
            _ => None,
        })
        .expect("real TextEdit must commit a color command");
    for channel in &color[..3] {
        assert!((*channel - 0.2158605).abs() < 0.000001);
    }
    assert_eq!(color[3], 1.0);
    assert_eq!(h.snapshot.paint.as_ref().unwrap().settings.color, original);
    assert_eq!(h.runtime.paint_hex_color.as_ref().unwrap().1, "#7caacb");
    assert!(h.replace_hex("#zz0080").is_empty());
    assert!(h.replace_hex("#+08080").is_empty());
    assert!(h.replace_hex("#80+080").is_empty());
    assert!(h.replace_hex("#é0808").is_empty());
    h.click("#7caacb");
    assert!(
        h.click("Round tip preview").is_empty(),
        "untouched field must not quantize precise RGB"
    );
}

#[test]
fn saved_brush_selection_requires_use_and_displays_only_acknowledged_current_brush() {
    for sculpt_mode in [false, true] {
        let mut h = Harness::new();
        if sculpt_mode {
            h.sculpt();
        } else {
            h.paint();
        }
        h.snapshot.saved_brushes.available = true;
        let entry = PaintBrushPresetInfo {
            id: 19,
            name: "Owned custom brush".into(),
        };
        if sculpt_mode {
            h.snapshot.saved_brushes.sculpt = vec![entry];
        } else {
            h.snapshot.saved_brushes.paint = vec![entry];
        }
        assert!(
            h.click("Owned custom brush").is_empty(),
            "choosing must not recall"
        );
        let commands = h.click(if sculpt_mode {
            "Use sculpt brush"
        } else {
            "Use paint brush"
        });
        assert!(commands.iter().any(|c| match c {
            UiToBevy::SculptCommand(SculptCommand::SelectSavedBrushPreset { preset_id: 19 }) =>
                sculpt_mode,
            UiToBevy::PaintCommand(PaintCommand::SelectSavedBrushPreset { preset_id: 19 }) =>
                !sculpt_mode,
            _ => false,
        }));
        assert_eq!(h.snapshot.saved_brushes.selected_paint, None);
        assert_eq!(h.snapshot.saved_brushes.selected_sculpt, None);
        if sculpt_mode {
            h.snapshot.saved_brushes.selected_sculpt = Some(19);
        } else {
            h.snapshot.saved_brushes.selected_paint = Some(19);
        }
        h.frame(vec![]);
        assert!(
            h.click("Current saved brush: Owned custom brush")
                .is_empty()
        );
        h.snapshot.saved_brushes.active = true;
        assert!(
            h.click(if sculpt_mode {
                "Use sculpt brush"
            } else {
                "Use paint brush"
            })
            .is_empty()
        );
    }
}

#[test]
fn canvas_settings_remain_editable_during_a_stroke_while_directuv_settings_are_locked() {
    let mut h = Harness::new();
    h.paint();
    h.snapshot.paint.as_mut().unwrap().target.active = true;
    h.snapshot.saved_brushes.active = true;
    assert!(h.click("Eraser").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::SetBlendMode {
            mode: BlendMode::Erase
        })
    )));
    assert!(h.click("Undo canvas stroke").is_empty());
    h.snapshot.paint.as_mut().unwrap().target.mode = PaintTarget::DirectUv;
    assert!(h.click("Eraser").is_empty());
    assert!(h.click("Undo surface stroke").is_empty());
}

#[test]
fn hex_draft_is_discarded_on_project_change_and_round_preview_uses_accepted_settings() {
    let mut h = Harness::new();
    h.paint();
    h.frame(vec![]);
    h.runtime.paint_hex_color.as_mut().unwrap().1 = "#ff0000".into();
    h.snapshot.project.generation = "another owned project".into();
    let (output, _) = h.frame(vec![]);
    assert_eq!(h.runtime.paint_hex_color.as_ref().unwrap().1, "#7caacb");
    fn preview(shape: &egui::epaint::Shape) -> Option<&egui::epaint::Mesh> {
        match shape {
            egui::epaint::Shape::Mesh(mesh) if mesh.vertices.len() == 65 => Some(mesh),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(preview),
            _ => None,
        }
    }
    let mesh = output
        .shapes
        .iter()
        .find_map(|s| preview(&s.shape))
        .expect("actual radial preview mesh");
    let expected = egui::Color32::from(egui::Rgba::from_rgba_unmultiplied(0.2, 0.4, 0.6, 0.75));
    assert_eq!(mesh.vertices[0].color, expected);
    assert_eq!(mesh.vertices[33].color, egui::Color32::TRANSPARENT);
    assert!(
        ((mesh.vertices[1].pos - mesh.vertices[0].pos).length() - 24.0 * 0.9 * 0.8).abs() < 0.001
    );
}
#[test]
fn mount_requests_authoritative_brush_and_document_state_once() {
    let mut h = Harness::new();
    let (_, commands) = h.frame(vec![]);
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, UiToBevy::RequestBrushState))
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, UiToBevy::ProjectCommand(ProjectCommand::GetState)))
    );
    assert!(h.frame(vec![]).1.is_empty());
    assert!(h.snapshot.paint.is_none());
    h.snapshot.edit_mode = EditMode::Sculpt;
    assert!(
        h.frame(vec![])
            .1
            .iter()
            .any(|c| matches!(c, UiToBevy::RequestBrushState))
    );
}
#[test]
fn actual_paint_widgets_route_history_tools_without_optimistic_state() {
    let mut h = Harness::new();
    h.paint();
    assert!(
        h.click("Undo canvas stroke")
            .iter()
            .any(|c| matches!(c, UiToBevy::PaintCommand(PaintCommand::Undo)))
    );
    assert!(
        h.click("Redo canvas stroke")
            .iter()
            .any(|c| matches!(c, UiToBevy::PaintCommand(PaintCommand::Redo)))
    );
    assert!(h.click("Eraser").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::SetBlendMode {
            mode: BlendMode::Erase
        })
    )));
    assert_eq!(
        h.snapshot.paint.as_ref().unwrap().settings.blend_mode,
        BlendMode::Normal
    );
    assert!(h.click("DirectUV surface").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::SetTarget {
            target: PaintTarget::DirectUv
        })
    )));
    assert_eq!(
        h.snapshot.paint.as_ref().unwrap().target.mode,
        PaintTarget::Canvas
    );
}
#[test]
fn active_paint_locks_history_and_target_but_can_cancel() {
    let mut h = Harness::new();
    h.paint();
    h.snapshot.paint.as_mut().unwrap().target.active = true;
    h.snapshot.paint.as_mut().unwrap().target.mode = PaintTarget::DirectUv;
    assert!(h.click("Undo surface stroke").is_empty());
    assert!(h.click("DirectUV surface").is_empty());
    assert!(h.click("Eraser").is_empty());
    assert!(
        h.click("Cancel current stroke")
            .iter()
            .any(|c| matches!(c, UiToBevy::PaintCommand(PaintCommand::CancelStroke)))
    );
}
#[test]
fn actual_sculpt_widgets_use_backend_counts_and_active_lock() {
    let mut h = Harness::new();
    h.sculpt();
    assert!(
        h.click("Undo sculpt stroke")
            .iter()
            .any(|c| matches!(c, UiToBevy::SculptCommand(SculptCommand::Undo)))
    );
    assert!(
        h.click("Redo sculpt stroke")
            .iter()
            .any(|c| matches!(c, UiToBevy::SculptCommand(SculptCommand::Redo)))
    );
    assert!(h.click("Grab").iter().any(|c| matches!(
        c,
        UiToBevy::SculptCommand(SculptCommand::SetTool {
            tool: SculptTool::Grab
        })
    )));
    assert_eq!(h.snapshot.sculpt.as_ref().unwrap().tool, SculptTool::Push);
    h.snapshot.sculpt_history.active = true;
    assert!(h.click("Undo sculpt stroke").is_empty());
    assert!(h.click("Redo sculpt stroke").is_empty());
    assert!(h.click("Grab").iter().any(|c| matches!(
        c,
        UiToBevy::SculptCommand(SculptCommand::SetTool {
            tool: SculptTool::Grab
        })
    )));
    h.snapshot.sculpt_history.active = false;
    h.snapshot.sculpt_history.redo_strokes = 0;
    assert!(h.click("Redo sculpt stroke").is_empty());
}
#[test]
fn document_new_freezes_exact_generation_and_waits_for_matching_receipt() {
    let mut h = Harness::new();
    h.project();
    h.runtime.project_dialog = Some(ProjectDialog::new(Operation::New, &h.snapshot));
    h.snapshot.project.generation = "42".into();
    let commands = h.click("Discard and create new");
    assert!(commands.iter().any(|c|matches!(c,UiToBevy::ProjectCommand(ProjectCommand::New{expected_generation,confirm_discard:true}) if expected_generation=="18446744073709551615")));
    assert!(h.runtime.modal_open());
    assert!(h.click("Working…").is_empty());
    assert!(h.click("Cancel").is_empty());
    apply_bevy_message(
        &mut h.snapshot,
        BevyToUi::ProjectOperationFinished {
            operation: "Save".into(),
            success: true,
            message: "saved".into(),
        },
    );
    h.frame(vec![]);
    assert!(h.runtime.modal_open());
    apply_bevy_message(
        &mut h.snapshot,
        BevyToUi::ProjectOperationFinished {
            operation: "New".into(),
            success: false,
            message: "stale owner".into(),
        },
    );
    h.frame(vec![]);
    assert!(h.runtime.modal_open());
    assert!(h.click("Cancel").is_empty());
    assert!(!h.runtime.modal_open());
}
#[test]
fn document_cancel_and_escape_send_no_destructive_command() {
    let mut h = Harness::new();
    h.project();
    h.frame(vec![]);
    h.runtime.project_dialog = Some(ProjectDialog::new(Operation::New, &h.snapshot));
    assert!(h.click("Cancel").is_empty());
    assert!(!h.runtime.modal_open());
    h.runtime.project_dialog = Some(ProjectDialog::new(Operation::New, &h.snapshot));
    h.frame(vec![]);
    let commands = h
        .frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }])
        .1;
    assert!(commands.is_empty());
    assert!(!h.runtime.modal_open());
}
#[test]
fn stale_project_receipt_does_not_close_new_submission() {
    let mut h = Harness::new();
    h.project();
    apply_bevy_message(
        &mut h.snapshot,
        BevyToUi::ProjectOperationFinished {
            operation: "New".into(),
            success: true,
            message: "old".into(),
        },
    );
    h.runtime.project_dialog = Some(ProjectDialog::new(Operation::New, &h.snapshot));
    h.click("Discard and create new");
    h.frame(vec![]);
    assert!(h.runtime.modal_open());
    apply_bevy_message(
        &mut h.snapshot,
        BevyToUi::ProjectOperationFinished {
            operation: "New".into(),
            success: true,
            message: "new".into(),
        },
    );
    h.frame(vec![]);
    assert!(!h.runtime.modal_open());
}
#[test]
fn modal_reports_full_viewport_for_native_input_arbitration() {
    let mut h = Harness::new();
    h.project();
    h.runtime.project_dialog = Some(ProjectDialog::new(Operation::New, &h.snapshot));
    h.frame(vec![]);
    assert!(h.runtime.ui_regions().contains(&egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1200.0, 1800.0)
    )));
}
#[test]
fn uv_mask_and_history_controls_obey_preview_conflict_and_active_guards() {
    let mut h = Harness::new();
    h.paint();
    h.snapshot.paint.as_mut().unwrap().target.uv_layers = Some(UvLayerState {
        receiver: Some(3),
        enabled: true,
        can_undo: true,
        can_redo: true,
        layers: vec![UvLayerInfo {
            id: 2,
            name: "Surface detail".into(),
            visible: true,
            opacity: 1.0,
            locked: false,
            is_active: true,
            blend_mode: UvLayerBlendMode::Normal,
            has_mask: true,
            mask_enabled: true,
            paint_target: UvLayerPaintTarget::Color,
        }],
        ..Default::default()
    });
    assert!(h.click("Paint mask").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::UvLayers {
            command: UvLayerCommand::PaintTarget {
                layer_id: 2,
                target: UvLayerPaintTarget::Mask
            }
        })
    )));
    assert!(h.click("Undo UV layer edit").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::UvLayers {
            command: UvLayerCommand::Undo
        })
    )));
    h.snapshot
        .paint
        .as_mut()
        .unwrap()
        .target
        .uv_layers
        .as_mut()
        .unwrap()
        .projection_preview = true;
    assert!(h.click("Paint mask").is_empty());
    assert!(h.click("Undo UV layer edit").is_empty());
    assert!(h.click("DirectUV surface").is_empty());
    h.snapshot.paint.as_mut().unwrap().target.active = true;
    assert!(
        h.click("Cancel UV preview")
            .iter()
            .any(|c| matches!(c, UiToBevy::PaintCommand(PaintCommand::CancelUvProjection)))
    );
    let uv = h
        .snapshot
        .paint
        .as_mut()
        .unwrap()
        .target
        .uv_layers
        .as_mut()
        .unwrap();
    uv.projection_preview = false;
    uv.conflicted = true;
    assert!(h.click("Remove mask").is_empty());
}
#[test]
fn linear_midtones_reach_the_actual_color_picker_without_double_conversion() {
    let mut h = Harness::new();
    h.paint();
    let (output, _) = h.frame(vec![]);
    let expected = egui::Color32::from(egui::Rgba::from_rgb(0.2, 0.4, 0.6));
    fn contains(shape: &egui::epaint::Shape, expected: egui::Color32) -> bool {
        match shape {
            egui::epaint::Shape::Rect(r) => r.fill == expected,
            egui::epaint::Shape::Vec(shapes) => shapes.iter().any(|s| contains(s, expected)),
            _ => false,
        }
    }
    assert!(output.shapes.iter().any(|s| contains(&s.shape, expected)));
    assert_eq!(
        h.snapshot.paint.as_ref().unwrap().settings.color,
        [0.2, 0.4, 0.6, 1.0]
    );
}
#[test]
fn matching_document_receipt_survives_an_unrelated_receipt_in_the_same_batch() {
    let mut h = Harness::new();
    h.project();
    h.runtime.project_dialog = Some(ProjectDialog::new(Operation::New, &h.snapshot));
    h.click("Discard and create new");
    apply_bevy_message(
        &mut h.snapshot,
        BevyToUi::ProjectOperationFinished {
            operation: "New".into(),
            success: true,
            message: "created".into(),
        },
    );
    apply_bevy_message(
        &mut h.snapshot,
        BevyToUi::ProjectOperationFinished {
            operation: "Save".into(),
            success: true,
            message: "other".into(),
        },
    );
    h.frame(vec![]);
    assert!(!h.runtime.modal_open());
}
#[test]
fn file_popup_publishes_its_real_capture_rectangle_outside_the_toolbar() {
    let mut h = Harness::new();
    h.project();
    h.click("File");
    h.frame(vec![]);
    assert!(
        h.runtime
            .ui_regions()
            .iter()
            .any(|r| r.max.y > 44.0 && r.max.x < 860.0 && r.height() > 40.0)
    );
}
#[test]
fn save_uses_the_acknowledged_owned_path_and_save_as_remains_available() {
    let mut h = Harness::new();
    h.project();
    h.snapshot.project.path = Some("/tmp/owned.pentimento.json".into());
    h.click("File");
    let commands = h.click("Save project");
    assert!(commands.iter().any(|c|matches!(c,UiToBevy::ProjectCommand(ProjectCommand::Save{path}) if path=="/tmp/owned.pentimento.json")));
    assert!(h.runtime.modal_open());
    apply_bevy_message(
        &mut h.snapshot,
        BevyToUi::ProjectOperationFinished {
            operation: "Save".into(),
            success: true,
            message: "saved".into(),
        },
    );
    h.frame(vec![]);
    h.click("File");
    assert!(h.click("Save As…").is_empty());
    assert!(h.runtime.modal_open());
}
#[test]
fn canvas_rename_refusal_and_undo_restore_the_authoritative_edit_field() {
    let mut h = Harness::new();
    h.paint();
    h.snapshot.layers = vec![LayerInfo {
        id: 2,
        name: "Accepted".into(),
        visible: true,
        opacity: 1.0,
        is_active: true,
    }];
    h.click("Canvas layers");
    h.frame(vec![]);
    h.runtime.canvas_layer_names.get_mut(&2).unwrap().1 = "Rejected draft".into();
    let commands = h.click("Rename");
    assert!(commands.iter().any(|c|matches!(c,UiToBevy::PaintCommand(PaintCommand::RenameLayer{layer_id:2,name}) if name=="Rejected draft")), "commands={commands:?}, draft={:?}", h.runtime.canvas_layer_names);
    assert_eq!(h.runtime.canvas_layer_names[&2].1, "Accepted");
    h.snapshot.layers[0].name = "Restored by backend".into();
    h.frame(vec![]);
    assert_eq!(h.runtime.canvas_layer_names[&2].1, "Restored by backend");
}

fn visible_text_rects(output: &egui::FullOutput, label: &str) -> Vec<egui::Rect> {
    fn find(
        shape: &egui::epaint::Shape,
        label: &str,
        clip: egui::Rect,
        rects: &mut Vec<egui::Rect>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                if clip.contains(rect.center()) {
                    rects.push(rect);
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    find(shape, label, clip, rects);
                }
            }
            _ => {}
        }
    }
    let mut rects = Vec::new();
    for clipped in &output.shapes {
        find(&clipped.shape, label, clipped.clip_rect, &mut rects);
    }
    rects
}

fn workspace_fixture(h: &mut Harness, sculpt: bool) {
    h.project();
    if sculpt {
        h.sculpt();
    } else {
        h.paint();
        h.snapshot.paint.as_mut().unwrap().presets =
            ["Round", "Soft", "Ink", "Airbrush", "Pencil", "Marker"]
                .into_iter()
                .enumerate()
                .map(|(id, name)| PaintBrushPresetInfo {
                    id: id as u32 + 7,
                    name: name.into(),
                })
                .collect();
        h.snapshot.layers = vec![
            LayerInfo {
                id: 1,
                name: "Base color".into(),
                visible: true,
                opacity: 1.0,
                is_active: false,
            },
            LayerInfo {
                id: 2,
                name: "Details".into(),
                visible: true,
                opacity: 0.8,
                is_active: true,
            },
        ];
    }
    h.snapshot.saved_brushes.available = true;
}

#[test]
fn narrow_and_wide_workspaces_keep_tools_and_layers_visible_without_overlap() {
    for size in [
        egui::vec2(640.0, 720.0),
        egui::vec2(800.0, 900.0),
        egui::vec2(1440.0, 900.0),
    ] {
        for sculpt in [false, true] {
            let mut h = Harness::new();
            h.size = size;
            workspace_fixture(&mut h, sculpt);
            h.frame(vec![]);
            let (output, _) = h.frame(vec![]);
            let labels: &[&str] = if sculpt {
                &[
                    "Pentimento",
                    "Sculpt brushes",
                    "Grab",
                    "Crease",
                    "Falloff",
                    "Auto smoothing %",
                    "Saved brushes",
                    "Undo sculpt stroke",
                ]
            } else {
                &[
                    "Pentimento",
                    "Brush presets",
                    "Round",
                    "Marker",
                    "Canvas layers",
                    "Details",
                    "Layer opacity %",
                    "Saved brushes",
                    "Radius (canvas px)",
                    "Undo canvas stroke",
                ]
            };
            let mut widgets = Vec::new();
            for label in labels {
                let rects = visible_text_rects(&output, label);
                assert!(
                    !rects.is_empty(),
                    "{label} must be visible without opening anything at {size:?}"
                );
                for rect in rects {
                    assert!(
                        egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect),
                        "{label} is outside {size:?}: {rect:?}"
                    );
                    assert!(
                        h.runtime
                            .ui_regions()
                            .iter()
                            .any(|region| region.contains_rect(rect)),
                        "{label} escapes the input capture contract"
                    );
                    widgets.push((label, rect));
                }
            }
            for (i, (label, rect)) in widgets.iter().enumerate() {
                for (other, other_rect) in &widgets[i + 1..] {
                    assert!(
                        rect.intersect(*other_rect).area() <= 0.0,
                        "{label} overlaps {other} at {size:?}"
                    );
                }
            }
            let regions = h.runtime.ui_regions();
            assert_eq!(
                regions.len(),
                4,
                "toolbar, context bar, shelf, properties dock"
            );
            for (i, rect) in regions.iter().enumerate() {
                for other in &regions[i + 1..] {
                    assert!(
                        rect.intersect(*other).area() <= 1.0,
                        "overlapping docks: {regions:?}"
                    );
                }
            }
            let viewport_point = if size.x < 1100.0 {
                egui::pos2(200.0, 300.0)
            } else {
                egui::pos2(700.0, 400.0)
            };
            assert!(
                !regions.iter().any(|rect| rect.contains(viewport_point)),
                "canvas must remain available"
            );
        }
    }
}

#[test]
fn shelf_reflows_on_resize_without_reopening_or_losing_dispatch() {
    let mut h = Harness::new();
    workspace_fixture(&mut h, true);
    for size in [
        egui::vec2(1440.0, 900.0),
        egui::vec2(800.0, 900.0),
        egui::vec2(1440.0, 900.0),
    ] {
        h.size = size;
        for _ in 0..2 {
            let commands = h.click("Grab");
            assert_eq!(
                commands
                    .iter()
                    .filter(|c| matches!(
                        c,
                        UiToBevy::SculptCommand(SculptCommand::SetTool {
                            tool: SculptTool::Grab
                        })
                    ))
                    .count(),
                1
            );
            assert_eq!(h.snapshot.sculpt.as_ref().unwrap().tool, SculptTool::Push);
        }
    }
}

#[test]
fn built_in_preset_tiles_dispatch_once_and_obey_the_owner_lock() {
    let mut h = Harness::new();
    workspace_fixture(&mut h, false);
    for _ in 0..3 {
        let commands = h.click("Marker");
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands[0],
            UiToBevy::PaintCommand(PaintCommand::SelectBrushPreset { preset_id: 12 })
        ));
        assert_eq!(h.snapshot.paint.as_ref().unwrap().settings.preset_id, 7);
    }
    h.snapshot.saved_brushes.active = true;
    assert!(h.click("Marker").is_empty());
    h.snapshot.saved_brushes.active = false;
    h.snapshot.paint.as_mut().unwrap().target.active = true;
    h.snapshot.paint.as_mut().unwrap().target.mode = PaintTarget::DirectUv;
    assert!(h.click("Marker").is_empty());
}

#[test]
fn canvas_stack_edits_only_the_acknowledged_selected_layer() {
    let mut h = Harness::new();
    workspace_fixture(&mut h, false);
    let (output, _) = h.frame(vec![]);
    assert_eq!(visible_text_rects(&output, "Layer opacity %").len(), 1);
    assert_eq!(visible_text_rects(&output, "Rename").len(), 1);
    assert!(h.click("Down").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::ReorderLayer {
            layer_id: 2,
            new_index: 0
        })
    )));
    assert!(h.click("Base color").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::SetActiveLayer { layer_id: 1 })
    )));
    assert!(h.snapshot.layers[1].is_active);
    // Receipt changes selection; subsequent actions must use that identity.
    h.snapshot.layers[0].is_active = true;
    h.snapshot.layers[1].is_active = false;
    assert!(h.click("Up").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::ReorderLayer {
            layer_id: 1,
            new_index: 1
        })
    )));
    assert!(h.click("Delete").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::RemoveLayer { layer_id: 1 })
    )));
    h.snapshot.paint.as_mut().unwrap().target.active = true;
    assert!(h.click("Delete").is_empty());
}

#[test]
fn uv_stack_uses_owner_order_and_selected_properties_after_repeated_switching() {
    let mut h = Harness::new();
    h.paint();
    h.snapshot.paint.as_mut().unwrap().target.mode = PaintTarget::DirectUv;
    h.snapshot.paint.as_mut().unwrap().target.uv_layers = Some(UvLayerState {
        receiver: Some(3),
        enabled: true,
        can_undo: true,
        layers: ["Top", "Middle", "Bottom"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| UvLayerInfo {
                id: index as u32 + 1,
                name: name.into(),
                visible: true,
                opacity: 1.0,
                locked: false,
                is_active: index == 1,
                blend_mode: UvLayerBlendMode::Normal,
                has_mask: true,
                mask_enabled: true,
                paint_target: UvLayerPaintTarget::Color,
            })
            .collect(),
        ..Default::default()
    });
    let (output, _) = h.frame(vec![]);
    assert_eq!(visible_text_rects(&output, "UV opacity %").len(), 1);
    assert_eq!(visible_text_rects(&output, "Rename UV layer").len(), 1);
    assert!(h.click("Move UV up").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::UvLayers {
            command: UvLayerCommand::Reorder {
                layer_id: 2,
                new_index: 2
            }
        })
    )));
    assert!(h.click("Move UV down").iter().any(|c| matches!(
        c,
        UiToBevy::PaintCommand(PaintCommand::UvLayers {
            command: UvLayerCommand::Reorder {
                layer_id: 2,
                new_index: 0
            }
        })
    )));
    for id in [1, 3, 2] {
        let name = ["Top", "Middle", "Bottom"][(id - 1) as usize];
        assert!(h.click(name).iter().any(|c| matches!(c, UiToBevy::PaintCommand(PaintCommand::UvLayers { command: UvLayerCommand::Select { layer_id } }) if *layer_id == id)));
        for layer in &mut h
            .snapshot
            .paint
            .as_mut()
            .unwrap()
            .target
            .uv_layers
            .as_mut()
            .unwrap()
            .layers
        {
            layer.is_active = layer.id == id;
        }
        assert!(h.click("Duplicate UV layer").iter().any(|c| matches!(c, UiToBevy::PaintCommand(PaintCommand::UvLayers { command: UvLayerCommand::Duplicate { layer_id } }) if *layer_id == id)));
    }
}

#[test]
fn full_stacks_and_catalogs_stay_bounded_and_preserve_toolbar_access() {
    let mut h = Harness::new();
    workspace_fixture(&mut h, false);
    h.size = egui::vec2(800.0, 900.0);
    h.snapshot.layers = (0..200)
        .map(|id| LayerInfo {
            id,
            name: format!("Layer {id}: a deliberately long layer name to test clipping"),
            visible: true,
            opacity: 1.0,
            is_active: id == 199,
        })
        .collect();
    h.snapshot.saved_brushes.paint = (0..64)
        .map(|id| PaintBrushPresetInfo {
            id,
            name: format!("Saved brush {id}"),
        })
        .collect();
    h.frame(vec![]);
    let (output, _) = h.frame(vec![]);
    assert_eq!(visible_text_rects(&output, "Layer opacity %").len(), 1);
    assert_eq!(visible_text_rects(&output, "Rename").len(), 1);
    assert!(!visible_text_rects(&output, "Add canvas layer").is_empty());
    assert!(
        h.click("Undo canvas stroke")
            .iter()
            .any(|c| matches!(c, UiToBevy::PaintCommand(PaintCommand::Undo)))
    );
    for region in h.runtime.ui_regions() {
        assert!(region.max.x <= h.size.x && region.max.y <= h.size.y);
    }
}

#[test]
fn capture_actual_egui_workspaces_when_requested() {
    let Ok(directory) = std::env::var("PENTIMENTO_EGUI_EVIDENCE_DIR") else {
        return;
    };
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    for (name, size, sculpt) in [
        ("paint-wide", egui::vec2(1440.0, 900.0), false),
        ("paint-narrow", egui::vec2(800.0, 900.0), false),
        ("sculpt-wide", egui::vec2(1440.0, 900.0), true),
        ("sculpt-narrow", egui::vec2(800.0, 900.0), true),
        ("uv-wide", egui::vec2(1440.0, 900.0), false),
        ("uv-narrow", egui::vec2(800.0, 900.0), false),
    ] {
        let mut h = Harness::new();
        h.size = size;
        workspace_fixture(&mut h, sculpt);
        let entries = ["Detail brush", "Soft shading"]
            .into_iter()
            .enumerate()
            .map(|(id, name)| PaintBrushPresetInfo {
                id: id as u32 + 30,
                name: name.into(),
            })
            .collect();
        if sculpt {
            h.snapshot.saved_brushes.sculpt = entries;
            h.snapshot.saved_brushes.selected_sculpt = Some(30);
        } else {
            h.snapshot.saved_brushes.paint = entries;
            h.snapshot.saved_brushes.selected_paint = Some(30);
        }
        if name.starts_with("uv-") {
            let target = &mut h.snapshot.paint.as_mut().unwrap().target;
            target.mode = PaintTarget::DirectUv;
            target.uv_layers = Some(UvLayerState {
                receiver: Some(3),
                enabled: true,
                can_undo: true,
                can_redo: true,
                receivers: vec![UvReceiverInfo {
                    mesh_id: 3,
                    name: "Sculpted mesh".into(),
                    layered: true,
                }],
                layers: ["Surface detail", "Base color"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| UvLayerInfo {
                        id: index as u32 + 1,
                        name: name.into(),
                        visible: true,
                        opacity: 1.0,
                        locked: false,
                        is_active: index == 0,
                        blend_mode: UvLayerBlendMode::Normal,
                        has_mask: index == 0,
                        mask_enabled: true,
                        paint_target: UvLayerPaintTarget::Color,
                    })
                    .collect(),
                ..Default::default()
            });
        }
        h.frame(vec![]);
        let (output, _) = h.frame(vec![]);
        h.textures
            .write_frame(&directory.join(format!("{name}.ppm")), &h.ctx, output, size);
    }
}

#[test]
fn startup_exposes_real_paint_creation_and_sculpt_entry_guidance() {
    let mut h = Harness::new();
    let (output, _) = h.frame(vec![]);
    assert!(!visible_text_rects(&output, "Sculpt a mesh").is_empty());
    assert!(!visible_text_rects(&output, "Select a mesh in the viewport, then press Ctrl+Tab to enter Sculpt. Ctrl+Tab leaves Sculpt.").is_empty());
    for _ in 0..2 {
        assert!(h.click("Paint Canvas").iter().any(|c| matches!(
            c,
            UiToBevy::AddPaintCanvas(AddPaintCanvasRequest {
                width: None,
                height: None
            })
        )));
        assert_eq!(
            h.snapshot.edit_mode,
            EditMode::None,
            "entry is never optimistic"
        );
    }
    assert!(h.click("Sphere").iter().any(|c| matches!(
        c,
        UiToBevy::AddObject(AddObjectRequest {
            primitive_type: PrimitiveType::Sphere,
            ..
        })
    )));
}

#[test]
fn numeric_toolbar_edits_dispatch_real_values_without_optimistic_snapshots() {
    fn enter_value(h: &mut Harness, accepted_text: &str, edited_text: &str) -> Vec<UiToBevy> {
        assert!(h.click(accepted_text).is_empty());
        // DragValue switches into its actual text editor on a click.
        h.frame(vec![]);
        assert!(h.ctx.egui_wants_keyboard_input());
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        h.frame(vec![
            key(
                egui::Key::A,
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            ),
            egui::Event::Text(edited_text.into()),
            key(egui::Key::Enter, Default::default()),
        ])
        .1
    }

    let mut paint = Harness::new();
    paint.paint();
    let accepted = paint.snapshot.paint.as_ref().unwrap().settings.clone();
    for (text, expected) in [("40", 0.4), ("55", 0.55)] {
        let commands = enter_value(&mut paint, "75.0", text);
        assert_eq!(commands.len(), 1, "commands={commands:?}");
        assert!(
            matches!(commands[0], UiToBevy::PaintCommand(PaintCommand::SetBrushOpacity { opacity }) if (opacity - expected).abs() < 0.000001)
        );
        assert_eq!(paint.snapshot.paint.as_ref().unwrap().settings, accepted);
        let (output, _) = paint.frame(vec![]);
        assert!(
            !visible_text_rects(&output, "75.0").is_empty(),
            "a pending/refused edit displays the accepted value again"
        );
    }
    // A backend receipt, rather than widget interaction, supplies the next value.
    paint.snapshot.paint.as_mut().unwrap().settings.opacity = 0.4;
    let commands = enter_value(&mut paint, "40.0", "80");
    assert!(
        matches!(commands.as_slice(), [UiToBevy::PaintCommand(PaintCommand::SetBrushOpacity { opacity })] if (*opacity - 0.8).abs() < 0.000001)
    );
    assert_eq!(paint.snapshot.paint.as_ref().unwrap().settings.opacity, 0.4);

    let mut sculpt = Harness::new();
    sculpt.sculpt();
    let accepted = sculpt.snapshot.sculpt.clone();
    for (text, expected) in [("2.25", 2.25), ("3.5", 3.5)] {
        let commands = enter_value(&mut sculpt, "1.0", text);
        assert_eq!(commands.len(), 1, "commands={commands:?}");
        assert!(
            matches!(commands[0], UiToBevy::SculptCommand(SculptCommand::SetRadius { radius }) if (radius - expected).abs() < 0.000001)
        );
        assert_eq!(sculpt.snapshot.sculpt, accepted);
        let (output, _) = sculpt.frame(vec![]);
        assert!(!visible_text_rects(&output, "1.0").is_empty());
    }
}
