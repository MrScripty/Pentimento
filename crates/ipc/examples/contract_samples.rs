use pentimento_ipc::{
    AddObjectRequest, AddPaintCanvasRequest, AmbientOcclusionSettings, AppSettings, BevyToUi,
    BlendMode, ColorSampleSource, DiffusionRequest, EditMode, GizmoCommand, GizmoMode,
    KeyboardEvent, LayerInfo, LightingSettings, MeshEditCommand, MeshEditTool, MeshSelectionMode,
    Modifiers, PaintBrushPresetInfo, PaintBrushSettings, PaintCommand, PaintTarget,
    PaintTargetState, PrimitiveType, SceneInfo, SceneObject, SculptBrushSettings, SculptCommand,
    SculptFalloff, SculptTool, Transform3D, UiToBevy, UvLayerCommand, UvLayerInfo, UvLayerState,
    UvReceiverInfo,
};
use serde::Serialize;

#[derive(Serialize)]
struct ContractSamples {
    bevy_to_ui: Vec<BevyToUi>,
    ui_to_bevy: Vec<UiToBevy>,
    native_keyboard: Vec<KeyboardEvent>,
}

fn main() {
    let samples = ContractSamples {
        native_keyboard: vec![
            KeyboardEvent {
                key: "3".into(),
                code: "Digit3".into(),
                text: Some("#".into()),
                pressed: true,
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            },
            KeyboardEvent {
                key: "Enter".into(),
                code: "Enter".into(),
                text: None,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
        bevy_to_ui: vec![
            BevyToUi::ProjectStateChanged {
                generation: "9007199254740993".into(),
                path: Some("/tmp/example.pentimento.json".into()),
                available: true,
                active: false,
                blocked: false,
                notice: Some("Saved".into()),
            },
            BevyToUi::ProjectOperationFinished {
                operation: "Save".into(),
                success: true,
                message: "Saved".into(),
            },
            BevyToUi::Initialize {
                scene_info: SceneInfo {
                    objects: vec![SceneObject {
                        id: "object-1".into(),
                        name: "Paint Canvas".into(),
                        transform: Transform3D::default(),
                        material_id: Some("material-1".into()),
                        visible: true,
                    }],
                    ..SceneInfo::default()
                },
                settings: AppSettings::default(),
            },
            BevyToUi::ShowAddObjectMenu {
                show: true,
                position: Some([128.0, 256.0]),
            },
            BevyToUi::AmbientOcclusionChanged {
                settings: AmbientOcclusionSettings::default(),
            },
            BevyToUi::EditModeChanged {
                mode: EditMode::Paint,
            },
            BevyToUi::MeshEditModeChanged {
                active: true,
                selection_mode: MeshSelectionMode::Face,
                tool: MeshEditTool::Inset,
            },
            BevyToUi::LayerStateChanged {
                layers: vec![
                    LayerInfo {
                        id: 1,
                        name: "Base".into(),
                        visible: true,
                        opacity: 1.0,
                        is_active: true,
                    },
                    LayerInfo {
                        id: 2,
                        name: "Highlights".into(),
                        visible: true,
                        opacity: 0.45,
                        is_active: false,
                    },
                ],
            },
            BevyToUi::PaintColorSamplingChanged {
                enabled: true,
                source: ColorSampleSource::VisibleLayers,
                active: false,
            },
            BevyToUi::PaintBrushStateChanged {
                settings: PaintBrushSettings {
                    preset_id: 0,
                    customized: true,
                    color: [0.0, 0.0, 0.0, 1.0],
                    size: 50.0,
                    opacity: 1.0,
                    hardness: 0.8,
                    spacing: 0.25,
                    blend_mode: BlendMode::Normal,
                },
                presets: vec![PaintBrushPresetInfo {
                    id: 0,
                    name: "Hard Round".into(),
                }],
                can_undo: true,
                can_redo: false,
                source_visible: true,
                target: PaintTargetState {
                    mode: PaintTarget::DirectUv,
                    direct_available: true,
                    target_name: Some("Sphere".into()),
                    active: false,
                    notice: None,
                    retained_bytes: 4096,
                    pending_bytes: 0,
                    limit_bytes: 67108864,
                    evicted_strokes: 0,
                    uv_layers: Some(UvLayerState {
                        receivers: vec![UvReceiverInfo {
                            mesh_id: 12,
                            name: "Sphere".into(),
                            layered: true,
                        }],
                        receiver: Some(12),
                        enabled: true,
                        layers: vec![UvLayerInfo {
                            id: 3,
                            name: "Detail".into(),
                            blend_mode: pentimento_ipc::UvLayerBlendMode::Overlay,
                            has_mask: true,
                            mask_enabled: true,
                            paint_target: pentimento_ipc::UvLayerPaintTarget::Mask,
                            visible: true,
                            opacity: 0.5,
                            locked: false,
                            is_active: true,
                        }],
                        can_undo: true,
                        can_redo: false,
                        active: false,
                        projection_preview: false,
                        conflicted: false,
                        notice: None,
                    }),
                },
            },
            BevyToUi::SculptBrushStateChanged {
                settings: Some(SculptBrushSettings {
                    tool: SculptTool::Push,
                    radius: 0.5,
                    strength: 1.0,
                    hardness: 0.5,
                    autosmooth: 0.5,
                    falloff: SculptFalloff::Smooth,
                }),
            },
            BevyToUi::SculptBrushStateChanged { settings: None },
            BevyToUi::SavedBrushPresetsChanged {
                paint: vec![PaintBrushPresetInfo {
                    id: 1,
                    name: "Ink detail".into(),
                }],
                sculpt: vec![PaintBrushPresetInfo {
                    id: 1,
                    name: "Gentle push".into(),
                }],
                selected_paint: Some(1),
                selected_sculpt: None,
                active: false,
                available: true,
                notice: None,
            },
            BevyToUi::SculptHistoryChanged {
                undo_strokes: 1,
                redo_strokes: 2,
                active: false,
                notice: None,
            },
            BevyToUi::ProjectionModeChanged {
                live_projection: true,
            },
            BevyToUi::CloseMenus,
        ],
        ui_to_bevy: vec![
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::SelectReceiver { mesh_id: 12 },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::Enable,
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::BlendMode {
                    layer_id: 3,
                    mode: pentimento_ipc::UvLayerBlendMode::Multiply,
                },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::AddMask { layer_id: 3 },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::RemoveMask { layer_id: 3 },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::MaskEnabled {
                    layer_id: 3,
                    enabled: false,
                },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::PaintTarget {
                    layer_id: 3,
                    target: pentimento_ipc::UvLayerPaintTarget::Mask,
                },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::Create {
                    name: "Detail".into(),
                },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::Select { layer_id: 3 },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::Reorder {
                    layer_id: 3,
                    new_index: 0,
                },
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::Undo,
            }),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::Redo,
            }),
            UiToBevy::ProjectCommand(pentimento_ipc::ProjectCommand::GetState),
            UiToBevy::ProjectCommand(pentimento_ipc::ProjectCommand::New {
                expected_generation: "9007199254740993".into(),
                confirm_discard: true,
            }),
            UiToBevy::ProjectCommand(pentimento_ipc::ProjectCommand::Save {
                path: "/tmp/example.pentimento.json".into(),
            }),
            UiToBevy::ProjectCommand(pentimento_ipc::ProjectCommand::Open {
                path: "/tmp/example.pentimento.json".into(),
            }),
            UiToBevy::SculptCommand(SculptCommand::Undo),
            UiToBevy::SculptCommand(SculptCommand::Redo),
            UiToBevy::PaintCommand(PaintCommand::SaveBrushPreset {
                name: "Ink detail".into(),
            }),
            UiToBevy::PaintCommand(PaintCommand::SelectSavedBrushPreset { preset_id: 1 }),
            UiToBevy::SculptCommand(SculptCommand::SaveBrushPreset {
                name: "Gentle push".into(),
            }),
            UiToBevy::SculptCommand(SculptCommand::SelectSavedBrushPreset { preset_id: 1 }),
            UiToBevy::AddObject(AddObjectRequest {
                primitive_type: PrimitiveType::Cube,
                position: Some([0.0, 1.0, 0.0]),
                name: Some("Blockout".into()),
            }),
            UiToBevy::UpdateLighting(LightingSettings::default()),
            UiToBevy::SetDepthView { enabled: true },
            UiToBevy::AddPaintCanvas(AddPaintCanvasRequest {
                width: Some(1024),
                height: Some(1024),
            }),
            UiToBevy::PaintCommand(PaintCommand::AddLayer {
                name: "Details".into(),
            }),
            UiToBevy::PaintCommand(PaintCommand::SetLayerOpacity {
                layer_id: 2,
                opacity: 0.45,
            }),
            UiToBevy::PaintCommand(PaintCommand::SetBrushSpacing { spacing: 0.2 }),
            UiToBevy::PaintCommand(PaintCommand::SetColorSampling { enabled: true }),
            UiToBevy::PaintCommand(PaintCommand::SetColorSampleSource {
                source: ColorSampleSource::ActiveLayer,
            }),
            UiToBevy::PaintCommand(PaintCommand::SetTarget {
                target: PaintTarget::DirectUv,
            }),
            UiToBevy::PaintCommand(PaintCommand::SetTarget {
                target: PaintTarget::Canvas,
            }),
            UiToBevy::PaintCommand(PaintCommand::CancelStroke),
            UiToBevy::PaintCommand(PaintCommand::CancelUvProjection),
            UiToBevy::PaintCommand(PaintCommand::Undo),
            UiToBevy::PaintCommand(PaintCommand::Redo),
            UiToBevy::PaintCommand(PaintCommand::SetSourceVisible { visible: false }),
            UiToBevy::PaintCommand(PaintCommand::ProjectToScene),
            UiToBevy::SculptCommand(SculptCommand::SetTool {
                tool: SculptTool::Grab,
            }),
            UiToBevy::SculptCommand(SculptCommand::SetRadius { radius: 0.8 }),
            UiToBevy::SculptCommand(SculptCommand::SetStrength { strength: 0.25 }),
            UiToBevy::SculptCommand(SculptCommand::SetHardness { hardness: 0.3 }),
            UiToBevy::SculptCommand(SculptCommand::SetAutoSmooth { amount: 0.2 }),
            UiToBevy::SculptCommand(SculptCommand::SetFalloff {
                falloff: SculptFalloff::Sharp,
            }),
            UiToBevy::RequestBrushState,
            UiToBevy::SetUiInputCapture { keyboard: true },
            UiToBevy::GizmoCommand(GizmoCommand::SetMode(GizmoMode::Translate)),
            UiToBevy::MeshEditCommand(MeshEditCommand::SetTool(MeshEditTool::Inset)),
            UiToBevy::StartDiffusion(DiffusionRequest {
                task_id: "task-1".into(),
                prompt: "weathered brass".into(),
                negative_prompt: Some("blurry".into()),
                width: 512,
                height: 512,
                steps: 24,
                guidance_scale: 7.0,
                seed: Some(7),
                target_material_slot: Some(("material-1".into(), "base_color".into())),
            }),
        ],
    };

    serde_json::to_writer_pretty(std::io::stdout(), &samples).expect("serialize contract samples");
    println!();
}
