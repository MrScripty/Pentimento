//! Mesh edit mode for vertex, edge, and face manipulation
//!
//! Provides Blender-style mesh editing:
//! - Tab to enter/exit edit mode
//! - 1/2/3 to switch between Vertex/Edge/Face selection modes
//! - Click to select, Shift+click to add to selection
//! - A to select all / deselect all

use bevy::ecs::message::Message;
use bevy::prelude::*;
use painting::half_edge::{FaceId, HalfEdgeId, HalfEdgeMesh, VertexId};
use pentimento_ipc::{BevyToUi, EditMode, MeshEditTool, MeshSelectionMode};
use std::collections::HashSet;

use crate::edit_mode::EditModeState;
use crate::canvas_plane::ActiveCanvasPlane;
use crate::OutboundUiMessages;
#[cfg(feature = "selection")]
use crate::selection::Selected;

/// Component marking a mesh as editable with half-edge topology data
#[derive(Component)]
pub struct EditableMesh {
    /// Half-edge mesh representation for topology operations
    pub half_edge_mesh: HalfEdgeMesh,
    /// Handle to the original Bevy mesh asset
    pub original_mesh_handle: Handle<Mesh>,
}

/// Resource tracking mesh edit mode state
#[derive(Resource, Default)]
pub struct MeshEditState {
    /// Current sub-object selection mode (vertex/edge/face)
    pub selection_mode: MeshSelectionMode,
    /// Current active tool
    pub tool: MeshEditTool,
    /// Entity currently being edited
    pub target_entity: Option<Entity>,
    /// Selected vertices
    pub selected_vertices: HashSet<VertexId>,
    /// Selected edges (stored as half-edge IDs)
    pub selected_edges: HashSet<HalfEdgeId>,
    /// Selected faces
    pub selected_faces: HashSet<FaceId>,
    /// Original vertex positions (for transform operations)
    pub original_positions: std::collections::HashMap<VertexId, Vec3>,
    /// Whether a transform operation is currently active
    pub transform_active: bool,
}

impl MeshEditState {
    /// Clear all selections
    pub fn clear_selection(&mut self) {
        self.selected_vertices.clear();
        self.selected_edges.clear();
        self.selected_faces.clear();
    }

    /// Get the total count of selected elements
    pub fn selection_count(&self) -> usize {
        self.selected_vertices.len() + self.selected_edges.len() + self.selected_faces.len()
    }

    /// Check if anything is selected
    pub fn has_selection(&self) -> bool {
        !self.selected_vertices.is_empty()
            || !self.selected_edges.is_empty()
            || !self.selected_faces.is_empty()
    }
}

/// Message for mesh edit mode transitions and commands
#[derive(Message, Debug, Clone)]
pub enum MeshEditEvent {
    /// Enter mesh edit mode for the specified entity
    Enter { entity: Entity },
    /// Exit mesh edit mode
    Exit,
    /// Set the selection mode (vertex/edge/face)
    SetSelectionMode(MeshSelectionMode),
    /// Set the active tool
    SetTool(MeshEditTool),
    /// Select all elements in the current mode
    SelectAll,
    /// Deselect all elements
    DeselectAll,
    /// Toggle select all (select if nothing selected, deselect if all selected)
    ToggleSelectAll,
}

/// Plugin for mesh edit mode
pub struct MeshEditModePlugin;

impl Plugin for MeshEditModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MeshEditState>()
            .add_message::<MeshEditEvent>()
            .add_systems(
                Update,
                (
                    handle_tab_key_for_mesh_edit,
                    handle_mesh_edit_events,
                    handle_selection_mode_hotkeys,
                )
                    .chain(),
            );
    }
}

/// Handle Tab key to toggle mesh edit mode
///
/// Tab behavior is context-aware:
/// - In Paint mode with canvas plane selected: handled by canvas_plane.rs (camera lock)
/// - Otherwise with mesh selected: toggle mesh edit mode
#[cfg(feature = "selection")]
fn handle_tab_key_for_mesh_edit(
    key_input: Res<ButtonInput<KeyCode>>,
    edit_mode: Res<EditModeState>,
    active_plane: Res<ActiveCanvasPlane>,
    mesh_edit_state: Res<MeshEditState>,
    selected_meshes: Query<Entity, (With<Selected>, With<Mesh3d>)>,
    mut events: MessageWriter<MeshEditEvent>,
) {
    if !key_input.just_pressed(KeyCode::Tab) {
        return;
    }

    // Don't handle Tab if we're in Paint mode with a canvas selected
    // (canvas_plane.rs handles that case for camera lock)
    if edit_mode.mode == EditMode::Paint && active_plane.entity.is_some() {
        return;
    }

    // If we're already in mesh edit mode, exit
    if edit_mode.mode == EditMode::MeshEdit {
        events.write(MeshEditEvent::Exit);
        return;
    }

    // If we have a mesh selected (not a canvas plane), enter mesh edit mode
    if let Ok(entity) = selected_meshes.single() {
        // Check that it's not the active canvas plane
        if active_plane.entity != Some(entity) {
            events.write(MeshEditEvent::Enter { entity });
        }
    }
}

/// Stub for non-selection builds
#[cfg(not(feature = "selection"))]
fn handle_tab_key_for_mesh_edit() {}

/// Handle mesh edit events
fn handle_mesh_edit_events(
    mut commands: Commands,
    mut events: MessageReader<MeshEditEvent>,
    mut edit_mode: ResMut<EditModeState>,
    mut mesh_edit_state: ResMut<MeshEditState>,
    mut outbound: ResMut<OutboundUiMessages>,
    meshes: Res<Assets<Mesh>>,
    mesh_query: Query<&Mesh3d>,
    editable_query: Query<&EditableMesh>,
) {
    for event in events.read() {
        match event {
            MeshEditEvent::Enter { entity } => {
                // Build EditableMesh if not already present
                if editable_query.get(*entity).is_err() {
                    if let Ok(mesh3d) = mesh_query.get(*entity) {
                        if let Some(mesh) = meshes.get(&mesh3d.0) {
                            match HalfEdgeMesh::from_bevy_mesh(mesh) {
                                Ok(half_edge_mesh) => {
                                    commands.entity(*entity).insert(EditableMesh {
                                        half_edge_mesh,
                                        original_mesh_handle: mesh3d.0.clone(),
                                    });
                                    info!("Built half-edge mesh for entity {:?}", entity);
                                }
                                Err(e) => {
                                    warn!("Failed to build half-edge mesh: {:?}", e);
                                    continue;
                                }
                            }
                        }
                    }
                }

                // Enter mesh edit mode
                edit_mode.mode = EditMode::MeshEdit;
                edit_mode.target_entity = Some(*entity);
                mesh_edit_state.target_entity = Some(*entity);
                mesh_edit_state.clear_selection();
                mesh_edit_state.selection_mode = MeshSelectionMode::Vertex;
                mesh_edit_state.tool = MeshEditTool::Select;

                info!("Entered mesh edit mode for entity {:?}", entity);

                // Notify UI
                outbound.send(BevyToUi::EditModeChanged {
                    mode: EditMode::MeshEdit,
                });
                outbound.send(BevyToUi::MeshEditModeChanged {
                    active: true,
                    selection_mode: mesh_edit_state.selection_mode,
                    tool: mesh_edit_state.tool,
                });
                outbound.send(BevyToUi::MeshEditSelectionChanged {
                    vertex_count: 0,
                    edge_count: 0,
                    face_count: 0,
                });
            }
            MeshEditEvent::Exit => {
                info!("Exited mesh edit mode");

                edit_mode.mode = EditMode::None;
                edit_mode.target_entity = None;
                mesh_edit_state.target_entity = None;
                mesh_edit_state.clear_selection();

                // Notify UI
                outbound.send(BevyToUi::EditModeChanged {
                    mode: EditMode::None,
                });
                outbound.send(BevyToUi::MeshEditModeChanged {
                    active: false,
                    selection_mode: MeshSelectionMode::Vertex,
                    tool: MeshEditTool::Select,
                });
            }
            MeshEditEvent::SetSelectionMode(mode) => {
                mesh_edit_state.selection_mode = *mode;
                info!("Set selection mode to {:?}", mode);

                outbound.send(BevyToUi::MeshEditModeChanged {
                    active: true,
                    selection_mode: *mode,
                    tool: mesh_edit_state.tool,
                });
            }
            MeshEditEvent::SetTool(tool) => {
                mesh_edit_state.tool = *tool;
                info!("Set tool to {:?}", tool);

                outbound.send(BevyToUi::MeshEditModeChanged {
                    active: true,
                    selection_mode: mesh_edit_state.selection_mode,
                    tool: *tool,
                });
            }
            MeshEditEvent::SelectAll => {
                if let Some(entity) = mesh_edit_state.target_entity {
                    if let Ok(editable) = editable_query.get(entity) {
                        select_all(&mut mesh_edit_state, &editable.half_edge_mesh);
                        send_selection_changed(&mesh_edit_state, &mut outbound);
                    }
                }
            }
            MeshEditEvent::DeselectAll => {
                mesh_edit_state.clear_selection();
                send_selection_changed(&mesh_edit_state, &mut outbound);
            }
            MeshEditEvent::ToggleSelectAll => {
                if let Some(entity) = mesh_edit_state.target_entity {
                    if let Ok(editable) = editable_query.get(entity) {
                        if mesh_edit_state.has_selection() {
                            mesh_edit_state.clear_selection();
                        } else {
                            select_all(&mut mesh_edit_state, &editable.half_edge_mesh);
                        }
                        send_selection_changed(&mesh_edit_state, &mut outbound);
                    }
                }
            }
        }
    }
}

/// Select all elements based on current selection mode
fn select_all(state: &mut MeshEditState, mesh: &HalfEdgeMesh) {
    match state.selection_mode {
        MeshSelectionMode::Vertex => {
            state.selected_vertices.clear();
            for v in mesh.vertices() {
                state.selected_vertices.insert(v.id);
            }
        }
        MeshSelectionMode::Edge => {
            state.selected_edges.clear();
            // Add each unique edge once (not both half-edges)
            let mut seen = HashSet::new();
            for he in mesh.half_edges() {
                if let Some(dest) = mesh.get_half_edge_dest(he.id) {
                    let key = if he.origin.0 < dest.0 {
                        (he.origin, dest)
                    } else {
                        (dest, he.origin)
                    };
                    if !seen.contains(&key) {
                        seen.insert(key);
                        state.selected_edges.insert(he.id);
                    }
                }
            }
        }
        MeshSelectionMode::Face => {
            state.selected_faces.clear();
            for f in mesh.faces() {
                state.selected_faces.insert(f.id);
            }
        }
    }
}

/// Send selection changed message to UI
fn send_selection_changed(state: &MeshEditState, outbound: &mut OutboundUiMessages) {
    outbound.send(BevyToUi::MeshEditSelectionChanged {
        vertex_count: state.selected_vertices.len(),
        edge_count: state.selected_edges.len(),
        face_count: state.selected_faces.len(),
    });
}

/// Handle hotkeys for selection mode (1/2/3) and select all (A)
fn handle_selection_mode_hotkeys(
    key_input: Res<ButtonInput<KeyCode>>,
    edit_mode: Res<EditModeState>,
    mut events: MessageWriter<MeshEditEvent>,
) {
    // Only handle in mesh edit mode
    if edit_mode.mode != EditMode::MeshEdit {
        return;
    }

    // 1/2/3 for selection modes
    if key_input.just_pressed(KeyCode::Digit1) {
        events.write(MeshEditEvent::SetSelectionMode(MeshSelectionMode::Vertex));
    } else if key_input.just_pressed(KeyCode::Digit2) {
        events.write(MeshEditEvent::SetSelectionMode(MeshSelectionMode::Edge));
    } else if key_input.just_pressed(KeyCode::Digit3) {
        events.write(MeshEditEvent::SetSelectionMode(MeshSelectionMode::Face));
    }

    // A for toggle select all
    if key_input.just_pressed(KeyCode::KeyA) {
        events.write(MeshEditEvent::ToggleSelectAll);
    }
}
