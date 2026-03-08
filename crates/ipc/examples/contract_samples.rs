use pentimento_ipc::{
    AddObjectRequest, AddPaintCanvasRequest, AmbientOcclusionSettings, AppSettings, BevyToUi,
    DiffusionRequest, EditMode, GizmoCommand, GizmoMode, LayerInfo, LightingSettings,
    MeshEditCommand, MeshEditTool, MeshSelectionMode, PaintCommand, PrimitiveType, SceneInfo,
    SceneObject, Transform3D, UiToBevy,
};
use serde::Serialize;

#[derive(Serialize)]
struct ContractSamples {
    bevy_to_ui: Vec<BevyToUi>,
    ui_to_bevy: Vec<UiToBevy>,
}

fn main() {
    let samples = ContractSamples {
        bevy_to_ui: vec![
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
            BevyToUi::CloseMenus,
        ],
        ui_to_bevy: vec![
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
