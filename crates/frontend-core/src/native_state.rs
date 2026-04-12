use pentimento_ipc::{BevyToUi, EditMode, LayerInfo, MeshEditTool, MeshSelectionMode};

/// Shared backend-derived snapshot used by native frontend implementations.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeUiState {
    pub show_add_menu: bool,
    pub add_menu_position: (f32, f32),
    pub edit_mode: EditMode,
    pub mesh_edit_active: bool,
    pub mesh_selection_mode: MeshSelectionMode,
    pub mesh_edit_tool: MeshEditTool,
    pub selected_vertex_count: usize,
    pub selected_edge_count: usize,
    pub selected_face_count: usize,
    pub depth_view_enabled: bool,
    pub layers: Vec<LayerInfo>,
}

impl Default for NativeUiState {
    fn default() -> Self {
        Self {
            show_add_menu: false,
            add_menu_position: (0.0, 0.0),
            edit_mode: EditMode::None,
            mesh_edit_active: false,
            mesh_selection_mode: MeshSelectionMode::Vertex,
            mesh_edit_tool: MeshEditTool::Select,
            selected_vertex_count: 0,
            selected_edge_count: 0,
            selected_face_count: 0,
            depth_view_enabled: false,
            layers: Vec::new(),
        }
    }
}

/// Apply a backend-originated message to the shared native frontend snapshot.
pub fn apply_native_ui_message(state: &mut NativeUiState, message: &BevyToUi) {
    match message {
        BevyToUi::ShowAddObjectMenu { show, position } => {
            state.show_add_menu = *show;
            if let Some([x, y]) = position {
                state.add_menu_position = (*x, *y);
            }
        }
        BevyToUi::EditModeChanged { mode } => {
            state.edit_mode = *mode;
        }
        BevyToUi::MeshEditModeChanged {
            active,
            selection_mode,
            tool,
        } => {
            state.mesh_edit_active = *active;
            state.mesh_selection_mode = *selection_mode;
            state.mesh_edit_tool = *tool;
        }
        BevyToUi::MeshEditSelectionChanged {
            vertex_count,
            edge_count,
            face_count,
        } => {
            state.selected_vertex_count = *vertex_count;
            state.selected_edge_count = *edge_count;
            state.selected_face_count = *face_count;
        }
        BevyToUi::CloseMenus => {
            state.show_add_menu = false;
        }
        BevyToUi::LayerStateChanged { layers } => {
            state.layers = layers.clone();
        }
        _ => {}
    }
}
