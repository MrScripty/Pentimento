//! Frontend-owned input arbitration state.
//!
//! Native frontends can use this resource to prevent viewport systems from
//! reacting while UI widgets own pointer or keyboard focus.

use bevy::prelude::*;

/// A native frontend's per-frame, chronologically arbitrated scene input.
/// UI-owned gestures never enter this stream; a scene gesture entering UI or
/// losing focus receives an explicit closing event. Global blocking flags
/// remain conservative for scene systems that do not consume this stream.
#[derive(Resource, Default)]
pub struct FrontendScenePointerInput {
    batch: Option<(Entity, Vec<bevy::window::WindowEvent>)>,
}

impl FrontendScenePointerInput {
    pub fn publish(&mut self, window: Entity, events: Vec<bevy::window::WindowEvent>) {
        self.batch = Some((window, events));
    }
    /// A backend switch cannot reinterpret a prefix already admitted by native input.
    pub fn has_scene_press(&self) -> bool {
        self.batch.as_ref().is_some_and(|(_, events)| {
            events.iter().any(|e| match e {
                bevy::window::WindowEvent::MouseButtonInput(e) => e.state.is_pressed(),
                bevy::window::WindowEvent::TouchInput(e) => {
                    e.phase == bevy::input::touch::TouchPhase::Started
                }
                _ => false,
            })
        })
    }
    pub fn clear(&mut self) {
        self.batch = None;
    }
    pub fn events(&self, window: Entity) -> Option<&[bevy::window::WindowEvent]> {
        self.batch
            .as_ref()
            .filter(|(owner, _)| *owner == window)
            .map(|(_, events)| events.as_slice())
    }
}

/// Native frontend's admitted keys for one frame, with modifiers at each event.
/// Published egui frames are authoritative even when empty. Other frontends
/// retain their existing raw-input route. Document replacement invalidates the
/// frame. Cursor origins travel with keys so later movement cannot rewrite F.
#[derive(Resource, Default)]
pub struct FrontendSceneKeyboardInput {
    generation: u64,
    enabled: bool,
    events: Vec<(pentimento_ipc::KeyboardEvent, Option<Vec2>)>,
    pointer_origin: (bool, bool, Option<Vec2>),
}
impl FrontendSceneKeyboardInput {
    pub fn begin_frame(&mut self, generation: u64, enabled: bool) {
        self.generation = generation;
        self.enabled = enabled;
        self.events.clear();
    }
    /// The native producer supplies only admitted, nonrepeat events.
    pub fn push(&mut self, event: pentimento_ipc::KeyboardEvent, cursor: Option<Vec2>) {
        self.events.push((event, cursor));
    }
    pub fn set_pointer_origin(&mut self, focused: bool, left_down: bool, cursor: Option<Vec2>) {
        self.pointer_origin = (focused, left_down, cursor);
    }
    pub(crate) fn pointer_origin(&self) -> (bool, bool, Option<Vec2>) {
        self.pointer_origin
    }
    pub fn events(
        &self,
        generation: u64,
    ) -> Option<&[(pentimento_ipc::KeyboardEvent, Option<Vec2>)]> {
        self.enabled.then(|| {
            if self.generation == generation {
                self.events.as_slice()
            } else {
                &[]
            }
        })
    }
}

/// Scene's native event dispatcher owns history when Sculpt is compiled in.
/// Prevents a key spanning a mode transition from reaching two brush owners.
#[derive(Resource, Default)]
pub struct NativeSceneHistoryOwner;

/// Private override used only while dispatching one admitted native segment.
/// Never rewrites the public input frame, Window or global ButtonInput state.
#[derive(Resource)]
pub(crate) struct NativePointerSegment {
    pub events: Vec<bevy::window::WindowEvent>,
    pub focused: bool,
    pub left_down: bool,
    pub cursor: Option<Vec2>,
    pub finish: bool,
    pub sampled: bool,
    pub reset_direct_contact: bool,
}

pub(crate) fn native_scene_managed(world: &World) -> bool {
    world.contains_resource::<NativeSceneHistoryOwner>()
        && world
            .get_resource::<FrontendSceneKeyboardInput>()
            .is_some_and(|input| input.events(crate::project_generation(world)).is_some())
}

/// Generic input blocking flags owned by the active frontend integration.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct FrontendInputBlockState {
    /// Prevent viewport and scene systems from consuming pointer input.
    pub block_pointer: bool,
    /// Prevent viewport and scene systems from consuming keyboard input.
    pub block_keyboard: bool,
}

impl FrontendInputBlockState {
    /// Returns true when scene systems should ignore pointer input.
    pub fn blocks_pointer(self) -> bool {
        self.block_pointer
    }

    /// Returns true when scene systems should ignore keyboard input.
    pub fn blocks_keyboard(self) -> bool {
        self.block_keyboard
    }
}

/// Browser-owned rectangles in webview coordinates; native and WASM hosts can
/// block viewport input before forwarding a click into an interactive widget.
#[derive(Resource, Default)]
pub struct FrontendUiLayout {
    pub regions: Vec<pentimento_ipc::LayoutRegion>,
    pub received: bool,
    pub pointer_captured: bool,
}

impl FrontendUiLayout {
    pub fn update_pointer(&mut self, x: f32, y: f32, down: bool, just_pressed: bool) -> bool {
        let over_ui = self
            .regions
            .iter()
            .any(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height);
        if just_pressed && over_ui {
            self.pointer_captured = true;
        }
        let blocked = over_ui || self.pointer_captured;
        if !down {
            self.pointer_captured = false;
        }
        blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widget_drag_stays_captured_until_release() {
        let mut layout = FrontendUiLayout {
            regions: vec![pentimento_ipc::LayoutRegion {
                id: "brush".into(),
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 20.0,
                z_index: 1,
                accepts_keyboard: true,
            }],
            received: true,
            ..default()
        };
        assert!(layout.update_pointer(15.0, 15.0, true, true));
        assert!(layout.update_pointer(200.0, 200.0, true, false));
        assert!(layout.update_pointer(200.0, 200.0, false, false));
        assert!(!layout.update_pointer(200.0, 200.0, false, false));
    }
}

/// Pressure supplied by Bevy/winit. Missing force is the ordinary full-pressure
/// pointer fallback; malformed supplied force is rejected, never promoted to 1.
pub fn touch_pressure(force: Option<bevy::input::touch::ForceTouch>) -> Option<f32> {
    use bevy::input::touch::ForceTouch;
    let pressure = match force {
        None => 1.,
        Some(ForceTouch::Normalized(value)) => value,
        Some(ForceTouch::Calibrated {
            force,
            max_possible_force,
            ..
        }) => {
            if !force.is_finite() || !max_possible_force.is_finite() || max_possible_force <= 0. {
                return None;
            }
            force / max_possible_force
        }
    };
    (pressure.is_finite() && (0.0..=1.0).contains(&pressure)).then_some(pressure as f32)
}
