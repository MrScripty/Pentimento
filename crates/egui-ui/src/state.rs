use pentimento_frontend_core::{NativeUiState, apply_native_ui_message};
use pentimento_ipc::{BevyToUi, EditMode};
use std::collections::BTreeMap;

pub type EguiUiSnapshot = NativeUiState;

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
    pub(crate) last_mode: Option<EditMode>,
    pub(crate) paint_preset_name: String,
    pub(crate) sculpt_preset_name: String,
    pub(crate) new_canvas_layer_name: String,
    pub(crate) new_uv_layer_name: String,
    pub(crate) canvas_layer_names: BTreeMap<u32, (String, String)>,
    pub(crate) uv_layer_names: BTreeMap<u32, (String, String)>,
    pub(crate) project_dialog: Option<crate::project_dialog::ProjectDialog>,
    pub(crate) project_requested: bool,
    pub(crate) ui_regions: Vec<egui::Rect>,
    last_generation: Option<String>,
    last_uv_receiver: Option<u32>,
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
            last_mode: None,
            paint_preset_name: String::new(),
            sculpt_preset_name: String::new(),
            new_canvas_layer_name: String::new(),
            new_uv_layer_name: String::new(),
            canvas_layer_names: BTreeMap::new(),
            uv_layer_names: BTreeMap::new(),
            project_dialog: None,
            project_requested: false,
            ui_regions: Vec::new(),
            last_generation: None,
            last_uv_receiver: None,
        }
    }
}

impl EguiUiRuntime {
    /// Actual panel/modal rectangles in egui points; the Bevy adapter converts to window coordinates.
    pub fn ui_regions(&self) -> &[egui::Rect] {
        &self.ui_regions
    }
    pub fn modal_open(&self) -> bool {
        self.project_dialog.is_some()
    }

    pub fn sync_from_snapshot(&mut self, snapshot: &mut EguiUiSnapshot) {
        if self.last_generation.as_ref() != Some(&snapshot.project.generation) {
            self.canvas_layer_names.clear();
            self.uv_layer_names.clear();
            self.last_generation = Some(snapshot.project.generation.clone());
        }
        let layers = snapshot
            .paint
            .as_ref()
            .and_then(|p| p.target.uv_layers.as_ref());
        let receiver = layers.and_then(|l| l.receiver);
        if self.last_uv_receiver != receiver {
            self.uv_layer_names.clear();
            self.last_uv_receiver = receiver;
        }
        self.canvas_layer_names
            .retain(|id, _| snapshot.layers.iter().any(|l| l.id == *id));
        self.uv_layer_names
            .retain(|id, _| layers.is_some_and(|s| s.layers.iter().any(|l| l.id == *id)));
        if snapshot.show_add_menu {
            self.add_object_menu_open = true;
            snapshot.show_add_menu = false;
        }
    }
}

pub fn apply_bevy_message(snapshot: &mut EguiUiSnapshot, message: BevyToUi) {
    apply_native_ui_message(snapshot, &message);
}
