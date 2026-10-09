use egui::{self, Align, Layout, Vec2};
use pentimento_ipc::{
    AddObjectRequest, AmbientOcclusionSettings, CameraCommand, EditMode, LightingSettings,
    PrimitiveType, ProjectCommand, UiToBevy,
};

use crate::state::{EguiUiRuntime, EguiUiSnapshot};

const TOP_PANEL_HEIGHT: f32 = 44.0;
const SIDE_PANEL_WIDTH: f32 = 340.0;
const WINDOW_PADDING: f32 = 12.0;
const ADD_MENU_DEFAULT_X: f32 = 24.0;
const ADD_MENU_DEFAULT_Y: f32 = 72.0;

pub fn show_root_ui(
    ctx: &egui::Context,
    snapshot: &mut EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
) -> Vec<UiToBevy> {
    runtime.sync_from_snapshot(snapshot);

    let mut commands = Vec::new();
    runtime.ui_regions.clear();
    if runtime.last_mode != Some(snapshot.edit_mode) {
        commands.push(UiToBevy::RequestBrushState);
        runtime.last_mode = Some(snapshot.edit_mode);
    }
    if !runtime.project_requested {
        commands.push(UiToBevy::ProjectCommand(ProjectCommand::GetState));
        runtime.project_requested = true;
    }

    draw_top_panel(ctx, snapshot, runtime, &mut commands);
    draw_side_panel(ctx, snapshot, runtime, &mut commands);
    draw_add_object_menu(ctx, snapshot, runtime, &mut commands);

    crate::project_dialog::show(ctx, snapshot, runtime, &mut commands);
    if runtime.modal_open() {
        runtime.ui_regions.push(ctx.viewport_rect());
    }
    // Menus and color-picker popups can extend beyond the fixed panels.
    // Include their actual areas in the same native hit-test contract.
    let floating = ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| matches!(layer.order, egui::Order::Middle | egui::Order::Foreground))
            .filter_map(|layer| memory.area_rect(layer.id))
            .collect::<Vec<_>>()
    });
    runtime.ui_regions.extend(floating);
    commands
}

fn draw_top_panel(
    ctx: &egui::Context,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    let panel = egui::TopBottomPanel::top("egui_toolbar")
        .exact_height(TOP_PANEL_HEIGHT)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Pentimento");
                crate::project_dialog::toolbar(ui, snapshot, runtime, commands);
                ui.separator();
                ui.label(edit_mode_label(snapshot.edit_mode));

                if snapshot.mesh_edit_active {
                    ui.separator();
                    ui.label(format!(
                        "{}:{}:{}  {:?}/{:?}",
                        snapshot.selected_vertex_count,
                        snapshot.selected_edge_count,
                        snapshot.selected_face_count,
                        snapshot.mesh_selection_mode,
                        snapshot.mesh_edit_tool
                    ));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let depth_toggle = if snapshot.depth_view_enabled {
                        "Depth View: On"
                    } else {
                        "Depth View: Off"
                    };

                    if ui.button(depth_toggle).clicked() {
                        commands.push(UiToBevy::SetDepthView {
                            enabled: !snapshot.depth_view_enabled,
                        });
                    }

                    if ui.button("Reset Camera").clicked() {
                        commands.push(UiToBevy::CameraCommand(CameraCommand::Reset));
                    }

                    if ui.button("Add").clicked() {
                        runtime.add_object_menu_open = true;
                    }
                });
            });
        });
    runtime.ui_regions.push(panel.response.rect);
}

fn draw_side_panel(
    ctx: &egui::Context,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    let panel = egui::SidePanel::right("egui_side_panel")
        .resizable(false)
        .exact_width(SIDE_PANEL_WIDTH)
        .show(ctx, |ui| {
            ui.add_space(WINDOW_PADDING);
            egui::ScrollArea::vertical().show(ui, |ui| {
                match snapshot.edit_mode {
                    EditMode::Paint => crate::paint_panel::show(ui, snapshot, runtime, commands),
                    EditMode::Sculpt => crate::sculpt_panel::show(ui, snapshot, runtime, commands),
                    _ => draw_scene_panel(ui, runtime, commands),
                }
                if let Some(error) = &snapshot.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            });
        });
    runtime.ui_regions.push(panel.response.rect);
}

fn draw_scene_panel(ui: &mut egui::Ui, runtime: &mut EguiUiRuntime, commands: &mut Vec<UiToBevy>) {
    ui.collapsing("Add Object", |ui| {
        let _ = primitive_buttons(ui, commands);

        if ui.button("Paint Canvas").clicked() {
            commands.push(UiToBevy::AddPaintCanvas(
                pentimento_ipc::AddPaintCanvasRequest {
                    width: None,
                    height: None,
                },
            ));
        }
    });

    ui.add_space(WINDOW_PADDING);

    ui.collapsing("Lighting", |ui| {
        let mut changed = false;
        changed |= ui
            .add(egui::Slider::new(&mut runtime.time_of_day, 0.0..=24.0).text("Time"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut runtime.cloudiness, 0.0..=100.0).text("Clouds"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut runtime.moon_phase_percent, 0.0..=100.0).text("Moon"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut runtime.azimuth_angle, 0.0..=360.0).text("Azimuth"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut runtime.pollution_percent, 0.0..=100.0).text("Pollution"))
            .changed();

        if changed {
            commands.push(UiToBevy::UpdateLighting(LightingSettings {
                time_of_day: runtime.time_of_day,
                cloudiness: runtime.cloudiness / 100.0,
                moon_phase: runtime.moon_phase_percent / 100.0,
                azimuth_angle: runtime.azimuth_angle,
                pollution: runtime.pollution_percent / 100.0,
                ..LightingSettings::default()
            }));
        }
    });

    ui.add_space(WINDOW_PADDING);

    ui.collapsing("Ambient Occlusion", |ui| {
        let mut changed = ui.checkbox(&mut runtime.ao_enabled, "Enabled").changed();
        changed |= ui
            .add(egui::Slider::new(&mut runtime.ao_quality_level, 0..=3).text("Quality"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut runtime.ao_thickness, 0.0625..=4.0).text("Thickness"))
            .changed();

        if changed {
            commands.push(UiToBevy::UpdateAmbientOcclusion(AmbientOcclusionSettings {
                enabled: runtime.ao_enabled,
                quality_level: runtime.ao_quality_level,
                constant_object_thickness: runtime.ao_thickness,
            }));
        }
    });
}

fn draw_add_object_menu(
    ctx: &egui::Context,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    if !runtime.add_object_menu_open {
        return;
    }

    let position = snapshot.add_menu_position;
    let position = egui::pos2(
        if position.0 == 0.0 {
            ADD_MENU_DEFAULT_X
        } else {
            position.0
        },
        if position.1 == 0.0 {
            ADD_MENU_DEFAULT_Y
        } else {
            position.1
        },
    );

    let mut open = runtime.add_object_menu_open;
    let mut should_close_menu = false;
    let response = egui::Window::new("Add Object")
        .default_pos(position)
        .default_size(Vec2::new(180.0, 220.0))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            should_close_menu |= primitive_buttons(ui, commands);

            ui.separator();

            if ui.button("Paint Canvas").clicked() {
                commands.push(UiToBevy::AddPaintCanvas(
                    pentimento_ipc::AddPaintCanvasRequest {
                        width: None,
                        height: None,
                    },
                ));
                should_close_menu = true;
            }
        });

    if let Some(response) = response {
        runtime.ui_regions.push(response.response.rect);
    }
    runtime.add_object_menu_open = open && !should_close_menu;
}

fn primitive_buttons(ui: &mut egui::Ui, commands: &mut Vec<UiToBevy>) -> bool {
    let mut activated = false;

    for primitive in [
        PrimitiveType::Cube,
        PrimitiveType::Sphere,
        PrimitiveType::Cylinder,
        PrimitiveType::Plane,
        PrimitiveType::Torus,
        PrimitiveType::Cone,
        PrimitiveType::Capsule,
    ] {
        if ui.button(format!("{primitive:?}")).clicked() {
            commands.push(UiToBevy::AddObject(AddObjectRequest {
                primitive_type: primitive,
                position: None,
                name: None,
            }));
            activated = true;
        }
    }

    activated
}

fn edit_mode_label(mode: EditMode) -> &'static str {
    match mode {
        EditMode::None => "Object Mode",
        EditMode::Paint => "Paint Mode",
        EditMode::MeshEdit => "Edit Mode",
        EditMode::Sculpt => "Sculpt Mode",
    }
}
