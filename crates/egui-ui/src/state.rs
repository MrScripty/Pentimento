use pentimento_ipc::{BevyToUi, EditMode, LayerInfo, MeshEditTool, MeshSelectionMode};

/// Snapshot of backend-owned state currently displayed by the egui frontend.
#[derive(Debug, Clone, Default)]
pub struct EguiUiSnapshot {
    pub show_add_object_menu: bool,
    pub add_object_menu_position: Option<[f32; 2]>,
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

/// UI-local state owned by the egui presentation layer.
#[derive(Debug, Clone)]
pub struct EguiUiRuntime {
    pub add_object_menu_open: bool,
    pub time_of_day: f32,
    pub cloudiness: f32,
    pub moon_phase_percent: f32,
    pub azimuth_angle: f32,
    pub pollution_percent: f32,
    pub ao_enabled: bool,
    pub ao_quality_level: u8,
    pub ao_thickness: f32,
    pub brush_size: f32,
    pub brush_opacity_percent: f32,
    pub brush_hardness_percent: f32,
    pub live_projection_enabled: bool,
}

impl Default for EguiUiRuntime {
    fn default() -> Self {
        Self {
            add_object_menu_open: false,
            time_of_day: 12.0,
            cloudiness: 0.0,
            moon_phase_percent: 50.0,
            azimuth_angle: 0.0,
            pollution_percent: 0.0,
            ao_enabled: false,
            ao_quality_level: 2,
            ao_thickness: 0.25,
            brush_size: 20.0,
            brush_opacity_percent: 100.0,
            brush_hardness_percent: 80.0,
            live_projection_enabled: false,
        }
    }
}

impl EguiUiRuntime {
    pub fn sync_from_snapshot(&mut self, snapshot: &EguiUiSnapshot) {
        if snapshot.show_add_object_menu {
            self.add_object_menu_open = true;
        }
    }
}

/// Apply a backend-originated message to the egui snapshot.
pub fn apply_bevy_message(snapshot: &mut EguiUiSnapshot, message: BevyToUi) {
    match message {
        BevyToUi::ShowAddObjectMenu { show, position } => {
            snapshot.show_add_object_menu = show;
            snapshot.add_object_menu_position = position;
        }
        BevyToUi::EditModeChanged { mode } => {
            snapshot.edit_mode = mode;
        }
        BevyToUi::MeshEditModeChanged {
            active,
            selection_mode,
            tool,
        } => {
            snapshot.mesh_edit_active = active;
            snapshot.mesh_selection_mode = selection_mode;
            snapshot.mesh_edit_tool = tool;
        }
        BevyToUi::MeshEditSelectionChanged {
            vertex_count,
            edge_count,
            face_count,
        } => {
            snapshot.selected_vertex_count = vertex_count;
            snapshot.selected_edge_count = edge_count;
            snapshot.selected_face_count = face_count;
        }
        BevyToUi::CloseMenus => {
            snapshot.show_add_object_menu = false;
            snapshot.add_object_menu_position = None;
        }
        BevyToUi::LayerStateChanged { layers } => {
            snapshot.layers = layers;
        }
        _ => {}
    }
}
