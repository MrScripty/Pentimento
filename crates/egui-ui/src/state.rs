use pentimento_frontend_core::{NativeUiState, apply_native_ui_message};
use pentimento_ipc::BevyToUi;

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
        if snapshot.show_add_menu {
            self.add_object_menu_open = true;
        }
    }
}

pub fn apply_bevy_message(snapshot: &mut EguiUiSnapshot, message: BevyToUi) {
    apply_native_ui_message(snapshot, &message);
}
