use pentimento_ipc::{
    BevyToUi, ColorSampleSource, EditMode, LayerInfo, MeshEditTool, MeshSelectionMode,
    PaintBrushPresetInfo, PaintBrushSettings, PaintTargetState, SculptBrushSettings,
};

#[derive(Debug, Clone, PartialEq)]
pub struct NativePaintState {
    pub settings: PaintBrushSettings,
    pub presets: Vec<PaintBrushPresetInfo>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub source_visible: bool,
    pub target: PaintTargetState,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NativeSculptHistory {
    pub undo_strokes: usize,
    pub redo_strokes: usize,
    pub active: bool,
    pub notice: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NativeSavedBrushState {
    pub paint: Vec<PaintBrushPresetInfo>,
    pub sculpt: Vec<PaintBrushPresetInfo>,
    pub selected_paint: Option<u32>,
    pub selected_sculpt: Option<u32>,
    pub active: bool,
    pub available: bool,
    pub notice: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NativeColorSampling {
    pub enabled: bool,
    pub source: ColorSampleSource,
    pub active: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct NativeProjectState {
    pub generation: String,
    pub path: Option<String>,
    pub available: bool,
    pub active: bool,
    pub blocked: bool,
    pub notice: Option<String>,
}
impl Default for NativeProjectState {
    fn default() -> Self {
        Self {
            generation: "0".into(),
            path: None,
            available: false,
            active: false,
            blocked: false,
            notice: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct NativeProjectReceipt {
    pub sequence: u64,
    pub operation: String,
    pub success: bool,
    pub message: String,
}

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
    pub paint: Option<NativePaintState>,
    pub sculpt: Option<SculptBrushSettings>,
    pub sculpt_received: bool,
    pub sculpt_history: NativeSculptHistory,
    pub saved_brushes: NativeSavedBrushState,
    pub color_sampling: NativeColorSampling,
    pub live_projection: bool,
    pub project: NativeProjectState,
    pub project_receipt: Option<NativeProjectReceipt>,
    /// One latest receipt per supported operation; a different operation cannot hide completion.
    pub project_receipts: [Option<NativeProjectReceipt>; 3],
    pub error: Option<String>,
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
            paint: None,
            sculpt: None,
            sculpt_received: false,
            sculpt_history: NativeSculptHistory::default(),
            saved_brushes: NativeSavedBrushState::default(),
            color_sampling: NativeColorSampling::default(),
            live_projection: false,
            project: NativeProjectState::default(),
            project_receipt: None,
            project_receipts: [None, None, None],
            error: None,
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
        BevyToUi::PaintBrushStateChanged {
            settings,
            presets,
            can_undo,
            can_redo,
            source_visible,
            target,
        } => {
            state.paint = Some(NativePaintState {
                settings: settings.clone(),
                presets: presets.clone(),
                can_undo: *can_undo,
                can_redo: *can_redo,
                source_visible: *source_visible,
                target: target.clone(),
            });
        }
        BevyToUi::SculptBrushStateChanged { settings } => {
            state.sculpt = settings.clone();
            state.sculpt_received = true;
        }
        BevyToUi::SculptHistoryChanged {
            undo_strokes,
            redo_strokes,
            active,
            notice,
        } => {
            state.sculpt_history = NativeSculptHistory {
                undo_strokes: *undo_strokes,
                redo_strokes: *redo_strokes,
                active: *active,
                notice: notice.clone(),
            };
        }
        BevyToUi::SavedBrushPresetsChanged {
            paint,
            sculpt,
            selected_paint,
            selected_sculpt,
            active,
            available,
            notice,
        } => {
            state.saved_brushes = NativeSavedBrushState {
                paint: paint.clone(),
                sculpt: sculpt.clone(),
                selected_paint: *selected_paint,
                selected_sculpt: *selected_sculpt,
                active: *active,
                available: *available,
                notice: notice.clone(),
            };
        }
        BevyToUi::PaintColorSamplingChanged {
            enabled,
            source,
            active,
        } => {
            state.color_sampling = NativeColorSampling {
                enabled: *enabled,
                source: *source,
                active: *active,
            };
        }
        BevyToUi::ProjectionModeChanged { live_projection } => {
            state.live_projection = *live_projection
        }
        BevyToUi::ProjectStateChanged {
            generation,
            path,
            available,
            active,
            blocked,
            notice,
        } => {
            state.project = NativeProjectState {
                generation: generation.clone(),
                path: path.clone(),
                available: *available,
                active: *active,
                blocked: *blocked,
                notice: notice.clone(),
            };
        }
        BevyToUi::ProjectOperationFinished {
            operation,
            success,
            message,
        } => {
            let sequence = state
                .project_receipt
                .as_ref()
                .map_or(1, |r| r.sequence.saturating_add(1));
            state.project_receipt = Some(NativeProjectReceipt {
                sequence,
                operation: operation.clone(),
                success: *success,
                message: message.clone(),
            });
            let slot = match operation.as_str() {
                "New" => Some(0),
                "Save" => Some(1),
                "Open" => Some(2),
                _ => None,
            };
            if let Some(slot) = slot {
                state.project_receipts[slot] = state.project_receipt.clone();
            }
        }
        BevyToUi::Error { message, .. } => state.error = Some(message.clone()),
        _ => {}
    }
}
