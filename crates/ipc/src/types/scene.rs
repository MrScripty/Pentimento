//! Scene-related types for IPC messages.

use serde::{Deserialize, Serialize};

/// Information about the current scene state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneInfo {
    pub objects: Vec<SceneObject>,
    pub cameras: Vec<CameraInfo>,
    pub lights: Vec<LightInfo>,
}

/// A scene object with its properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneObject {
    pub id: String,
    pub name: String,
    pub transform: Transform3D,
    pub material_id: Option<String>,
    pub visible: bool,
}

/// 3D transform with position, rotation, and scale.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transform3D {
    pub position: [f32; 3],
    pub rotation: [f32; 4], // Quaternion (x, y, z, w)
    pub scale: [f32; 3],
}

/// Camera information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraInfo {
    pub id: String,
    pub name: String,
    pub transform: Transform3D,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

/// Light information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightInfo {
    pub id: String,
    pub name: String,
    pub light_type: LightType,
    pub color: [f32; 3],
    pub intensity: f32,
    pub transform: Transform3D,
}

/// Type of light source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LightType {
    Directional,
    Point { range: f32 },
    Spot { range: f32, inner_angle: f32, outer_angle: f32 },
}

/// Primitive mesh types for object creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveType {
    Cube,
    Sphere,
    Cylinder,
    Plane,
    Torus,
    Cone,
    Capsule,
}

/// Request to add a new object to the scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddObjectRequest {
    pub primitive_type: PrimitiveType,
    /// Optional world position (defaults to origin)
    pub position: Option<[f32; 3]>,
    /// Optional custom name
    pub name: Option<String>,
}

/// Layout information for UI regions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayoutInfo {
    pub regions: Vec<LayoutRegion>,
}

/// A rectangular UI region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutRegion {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub z_index: i32,
    pub accepts_keyboard: bool,
}
