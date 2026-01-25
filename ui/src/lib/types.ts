/**
 * TypeScript types matching the Rust IPC protocol
 */

// Edit mode
export type EditMode = 'None' | 'Paint';

// Messages from Bevy to UI
export type BevyToUi =
    | { type: 'Initialize'; data: { scene_info: SceneInfo; settings: AppSettings } }
    | { type: 'SceneUpdated'; data: SceneInfo }
    | { type: 'SelectionChanged'; data: { selected_ids: string[] } }
    | { type: 'MaterialUpdated'; data: { material_id: string; properties: MaterialProperties } }
    | { type: 'DiffusionProgress'; data: { task_id: string; progress: number; preview_available: boolean } }
    | { type: 'DiffusionComplete'; data: { task_id: string; texture_id: string } }
    | { type: 'RenderStats'; data: { fps: number; frame_time_ms: number; draw_calls: number; triangles: number } }
    | { type: 'MouseEnter'; data: { region_id: string } }
    | { type: 'MouseLeave'; data: { region_id: string } }
    | { type: 'EditModeChanged'; data: { mode: EditMode } }
    | { type: 'Error'; data: { code: string; message: string } };

// Messages from UI to Bevy
export type UiToBevy =
    | { type: 'UiDirty' }
    | { type: 'LayoutUpdate'; data: LayoutInfo }
    | { type: 'CameraCommand'; data: CameraCommand }
    | { type: 'ObjectCommand'; data: ObjectCommand }
    | { type: 'MaterialCommand'; data: MaterialCommand }
    | { type: 'StartDiffusion'; data: DiffusionRequest }
    | { type: 'CancelDiffusion'; data: { task_id: string } }
    | { type: 'UpdateSettings'; data: AppSettings }
    | { type: 'NodeGraphUpdate'; data: NodeGraphState }
    | { type: 'UpdateLighting'; data: LightingSettings }
    | { type: 'UpdateAmbientOcclusion'; data: AmbientOcclusionSettings }
    | { type: 'AddObject'; data: AddObjectRequest }
    | { type: 'AddPaintCanvas'; data: { width: number | null; height: number | null } };

// Scene types
export interface SceneInfo {
    objects: SceneObject[];
    cameras: CameraInfo[];
    lights: LightInfo[];
}

export interface SceneObject {
    id: string;
    name: string;
    transform: Transform3D;
    material_id: string | null;
    visible: boolean;
}

export interface Transform3D {
    position: [number, number, number];
    rotation: [number, number, number, number];
    scale: [number, number, number];
}

export interface CameraInfo {
    id: string;
    name: string;
    transform: Transform3D;
    fov: number;
    near: number;
    far: number;
}

export interface LightInfo {
    id: string;
    name: string;
    light_type: LightType;
    color: [number, number, number];
    intensity: number;
    transform: Transform3D;
}

export type LightType =
    | { Directional: null }
    | { Point: { range: number } }
    | { Spot: { range: number; inner_angle: number; outer_angle: number } };

// Material types
export interface MaterialProperties {
    base_color: [number, number, number, number];
    metallic: number;
    roughness: number;
    emissive: [number, number, number];
    texture_slots: TextureSlot[];
}

export interface TextureSlot {
    slot_name: string;
    texture_id: string | null;
}

// Layout types
export interface LayoutInfo {
    regions: LayoutRegion[];
}

export interface LayoutRegion {
    id: string;
    x: number;
    y: number;
    width: number;
    height: number;
    z_index: number;
    accepts_keyboard: boolean;
}

// Command types
export type CameraCommand =
    | { Orbit: { delta_x: number; delta_y: number } }
    | { Pan: { delta_x: number; delta_y: number } }
    | { Zoom: { delta: number } }
    | { SetPosition: { position: [number, number, number] } }
    | { SetTarget: { target: [number, number, number] } }
    | { Reset: null };

export type ObjectCommand =
    | { Select: { ids: string[] } }
    | { Deselect: { ids: string[] } }
    | { Delete: { ids: string[] } }
    | { Duplicate: { ids: string[] } }
    | { Transform: { id: string; transform: Transform3D } }
    | { SetVisibility: { id: string; visible: boolean } }
    | { Rename: { id: string; name: string } };

export type MaterialCommand =
    | { UpdateProperty: { material_id: string; property: string; value: unknown } }
    | { AssignTexture: { material_id: string; slot: string; texture_id: string } }
    | { Create: { name: string } }
    | { Delete: { material_id: string } };

// Diffusion types
export interface DiffusionRequest {
    task_id: string;
    prompt: string;
    negative_prompt: string | null;
    width: number;
    height: number;
    steps: number;
    guidance_scale: number;
    seed: number | null;
    target_material_slot: [string, string] | null;
}

// Settings types
export interface AppSettings {
    render_scale: number;
    vsync: boolean;
    msaa_samples: number;
    show_wireframe: boolean;
    show_grid: boolean;
    diffusion_server_url: string | null;
}

// Node graph types
export interface NodeGraphState {
    nodes: NodeInfo[];
    connections: NodeConnection[];
}

export interface NodeInfo {
    id: string;
    node_type: string;
    position: [number, number];
    data: unknown;
}

export interface NodeConnection {
    from_node: string;
    from_output: string;
    to_node: string;
    to_input: string;
}

// Lighting settings
export interface LightingSettings {
    sun_direction: [number, number, number];
    sun_color: [number, number, number];
    sun_intensity: number;
    ambient_color: [number, number, number];
    ambient_intensity: number;
    time_of_day: number;
    cloudiness: number;
    use_time_of_day: boolean;
}

// Ambient occlusion settings
export interface AmbientOcclusionSettings {
    enabled: boolean;
    quality_level: number;
    constant_object_thickness: number;
}

// Add object request
export interface AddObjectRequest {
    primitive_type: string;
    position: [number, number, number] | null;
    name: string | null;
}
