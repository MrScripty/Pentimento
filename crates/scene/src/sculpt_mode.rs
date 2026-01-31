//! Sculpt mode for 3D mesh sculpting with dynamic tessellation
//!
//! Provides sculpting functionality:
//! - Ctrl+Tab to enter/exit sculpt mode (requires mesh selected)
//! - Brush-based deformation (Push, Pull, Smooth, etc.)
//! - Screen-space adaptive tessellation
//! - Mesh chunking for optimized GPU updates

use bevy::ecs::message::Message;
use bevy::prelude::*;
use pentimento_ipc::{BevyToUi, EditMode};
use sculpting::{ChunkConfig, DeformationType, TessellationConfig};

use crate::edit_mode::EditModeState;
use crate::OutboundUiMessages;
#[cfg(feature = "selection")]
use crate::selection::Selected;

/// Resource tracking sculpt mode state
#[derive(Resource)]
pub struct SculptState {
    /// Whether sculpt mode is currently active
    pub active: bool,
    /// Entity currently being sculpted
    pub target_entity: Option<Entity>,
    /// Current deformation type
    pub deformation_type: DeformationType,
    /// Brush radius in world units
    pub brush_radius: f32,
    /// Brush strength (0.0 - 1.0)
    pub brush_strength: f32,
    /// Tessellation configuration
    pub tessellation_config: TessellationConfig,
    /// Chunk sizing configuration
    pub chunk_config: ChunkConfig,
    /// Current stroke ID (if stroke in progress)
    pub current_stroke_id: Option<u64>,
}

impl Default for SculptState {
    fn default() -> Self {
        Self {
            active: false,
            target_entity: None,
            deformation_type: DeformationType::Push,
            brush_radius: 0.1,
            brush_strength: 0.5,
            tessellation_config: TessellationConfig::default(),
            chunk_config: ChunkConfig::default(),
            current_stroke_id: None,
        }
    }
}

/// Message for sculpt mode events
#[derive(Message, Debug, Clone)]
pub enum SculptEvent {
    /// Enter sculpt mode for the specified entity
    Enter { entity: Entity },
    /// Exit sculpt mode
    Exit,
    /// Set the deformation type
    SetDeformationType(DeformationType),
    /// Set brush radius
    SetBrushRadius(f32),
    /// Set brush strength
    SetBrushStrength(f32),
    /// Start a sculpt stroke
    StrokeStart {
        /// World-space position where stroke started
        world_pos: Vec3,
        /// Surface normal at hit point
        normal: Vec3,
        /// Unique stroke ID
        stroke_id: u64,
    },
    /// Continue a sculpt stroke
    StrokeMove {
        /// World-space position
        world_pos: Vec3,
        /// Surface normal at hit point
        normal: Vec3,
        /// Pressure value (0.0-1.0)
        pressure: f32,
    },
    /// End a sculpt stroke
    StrokeEnd,
    /// Cancel a sculpt stroke
    StrokeCancel,
}

/// Plugin for sculpt mode functionality
pub struct SculptModePlugin;

impl Plugin for SculptModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SculptState>()
            .add_message::<SculptEvent>()
            .add_systems(
                Update,
                (
                    handle_sculpt_mode_hotkey,
                    handle_sculpt_events,
                )
                    .chain(),
            );
    }
}

/// Handle Ctrl+Tab to toggle sculpt mode
///
/// Ctrl+Tab enters sculpt mode when a mesh is selected.
/// If already in sculpt mode, Ctrl+Tab exits.
#[cfg(feature = "selection")]
fn handle_sculpt_mode_hotkey(
    key_input: Res<ButtonInput<KeyCode>>,
    edit_mode: Res<EditModeState>,
    selected_meshes: Query<Entity, (With<Selected>, With<Mesh3d>)>,
    mut events: MessageWriter<SculptEvent>,
) {
    // Check for Ctrl modifier
    let ctrl = key_input.pressed(KeyCode::ControlLeft)
        || key_input.pressed(KeyCode::ControlRight);
    let tab = key_input.just_pressed(KeyCode::Tab);

    if !ctrl || !tab {
        return;
    }

    // If already in sculpt mode, exit
    if edit_mode.mode == EditMode::Sculpt {
        events.write(SculptEvent::Exit);
        return;
    }

    // If we have a mesh selected, enter sculpt mode
    if let Ok(entity) = selected_meshes.single() {
        events.write(SculptEvent::Enter { entity });
    }
}

/// Stub for non-selection builds
#[cfg(not(feature = "selection"))]
fn handle_sculpt_mode_hotkey() {}

/// Handle sculpt mode events
fn handle_sculpt_events(
    mut events: MessageReader<SculptEvent>,
    mut edit_mode: ResMut<EditModeState>,
    mut sculpt_state: ResMut<SculptState>,
    mut outbound: ResMut<OutboundUiMessages>,
) {
    for event in events.read() {
        match event {
            SculptEvent::Enter { entity } => {
                // Enter sculpt mode
                edit_mode.mode = EditMode::Sculpt;
                edit_mode.target_entity = Some(*entity);
                sculpt_state.active = true;
                sculpt_state.target_entity = Some(*entity);
                sculpt_state.deformation_type = DeformationType::Push;

                info!("Entered sculpt mode for entity {:?}", entity);

                // Notify UI
                outbound.send(BevyToUi::EditModeChanged {
                    mode: EditMode::Sculpt,
                });

                // TODO: Initialize chunked mesh from entity
                // TODO: Hide original mesh, show chunks
            }
            SculptEvent::Exit => {
                info!("Exited sculpt mode");

                // TODO: Merge chunks back into single mesh
                // TODO: Hide chunks, show original mesh

                edit_mode.mode = EditMode::None;
                edit_mode.target_entity = None;
                sculpt_state.active = false;
                sculpt_state.target_entity = None;
                sculpt_state.current_stroke_id = None;

                // Notify UI
                outbound.send(BevyToUi::EditModeChanged {
                    mode: EditMode::None,
                });
            }
            SculptEvent::SetDeformationType(deformation_type) => {
                sculpt_state.deformation_type = *deformation_type;
                info!("Set deformation type to {:?}", deformation_type);
            }
            SculptEvent::SetBrushRadius(radius) => {
                sculpt_state.brush_radius = radius.max(0.01);
                info!("Set brush radius to {}", sculpt_state.brush_radius);
            }
            SculptEvent::SetBrushStrength(strength) => {
                sculpt_state.brush_strength = strength.clamp(0.0, 1.0);
                info!("Set brush strength to {}", sculpt_state.brush_strength);
            }
            SculptEvent::StrokeStart {
                world_pos,
                normal,
                stroke_id,
            } => {
                sculpt_state.current_stroke_id = Some(*stroke_id);
                info!(
                    "Sculpt stroke started: id={}, pos={:?}, normal={:?}",
                    stroke_id, world_pos, normal
                );

                // TODO: Begin stroke recording
                // TODO: Capture undo checkpoint if needed
            }
            SculptEvent::StrokeMove {
                world_pos,
                normal,
                pressure,
            } => {
                // TODO: Generate dab
                // TODO: Apply deformation to affected vertices
                // TODO: Trigger tessellation around brush
                // TODO: Sync boundary vertices
                // TODO: Mark affected chunks dirty
                let _ = (world_pos, normal, pressure); // Silence unused warnings for now
            }
            SculptEvent::StrokeEnd => {
                if let Some(stroke_id) = sculpt_state.current_stroke_id.take() {
                    info!("Sculpt stroke ended: id={}", stroke_id);

                    // TODO: Finalize stroke recording
                    // TODO: Rebalance chunks if needed
                    // TODO: Save checkpoint for undo
                }
            }
            SculptEvent::StrokeCancel => {
                if let Some(stroke_id) = sculpt_state.current_stroke_id.take() {
                    info!("Sculpt stroke cancelled: id={}", stroke_id);

                    // TODO: Restore from last checkpoint
                }
            }
        }
    }
}
