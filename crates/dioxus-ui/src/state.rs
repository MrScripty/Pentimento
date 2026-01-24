//! Reactive state management for the Dioxus UI

use pentimento_ipc::{AppSettings, MaterialProperties, SceneInfo};
use serde::{Deserialize, Serialize};

/// Render statistics displayed in the toolbar
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RenderStats {
    pub fps: f32,
    pub frame_time: f32,
}

/// Main UI state synchronized with Bevy
#[derive(Debug, Clone, Default)]
pub struct UiState {
    /// Current render statistics
    pub render_stats: RenderStats,
    /// Scene information (objects, cameras, lights)
    pub scene_info: SceneInfo,
    /// Currently selected object IDs
    pub selected_ids: Vec<String>,
    /// Material properties for selected object
    pub selected_material: Option<MaterialProperties>,
    /// Application settings
    pub settings: AppSettings,
    /// Whether the UI needs to be recaptured
    pub dirty: bool,
}

impl UiState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn clear_dirty(&mut self) -> bool {
        let was_dirty = self.dirty;
        self.dirty = false;
        was_dirty
    }
}
