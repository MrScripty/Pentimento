//! Mesh painting system for Bevy integration
//!
//! This module connects MeshPaintEvent messages to mesh painting surfaces
//! and handles GPU texture upload for painted meshes.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use std::collections::{HashMap, HashSet};

use painting::BrushPreset;
use painting::mesh_surface::{MeshPtexSurface, MeshUvSurface};
use painting::types::{BlendMode, MeshHit, MeshStorageMode};

use crate::mesh_paint_mode::{MeshPaintEvent, PaintableMesh};
use crate::mesh_uv_history::{UvEntry, UvHistory, same_pixels};

struct UvProjectionPreview {
    mesh_id: u32,
    layer: u32,
    pixels: Vec<[f32; 4]>,
}

struct ActiveMeshStroke {
    entity: Entity,
    mesh_id: u32,
    before: Option<Vec<[f32; 4]>>,
    dimensions: (u32, u32),
    before_bound: bool,
    layer: Option<u32>,
    brush: painting::brush::BrushEngine,
    last_face: Option<u32>,
    color: [f32; 4],
    blend: BlendMode,
}

#[derive(Clone, PartialEq, Eq)]
struct MeshOwner {
    entity: Entity,
    mesh: bevy::asset::AssetId<Mesh>,
    storage: MeshStorageMode,
    image: bevy::asset::AssetId<Image>,
}

/// Resource holding painting surfaces for each paintable mesh
#[derive(Resource)]
pub struct MeshPaintingResource {
    /// UV-based surfaces indexed by mesh_id
    uv_surfaces: HashMap<u32, MeshUvSurface>,
    pub(crate) uv_layers: HashMap<u32, painting::uv_layers::UvLayers>,
    pub(crate) shared_display_bytes: HashMap<u32, Vec<u8>>,
    pub(crate) shared_original_bytes: HashMap<u32, Option<Vec<u8>>>,
    /// Ptex-based surfaces indexed by mesh_id
    ptex_surfaces: HashMap<u32, MeshPtexSurface>,
    /// Current brush color
    pub brush_color: [f32; 4],
    /// Current brush preset
    pub brush_preset: BrushPreset,
    /// Current blend mode
    pub blend_mode: BlendMode,
    active_stroke: Option<ActiveMeshStroke>,
    projection_preview: Option<UvProjectionPreview>,
    history: UvHistory,
    owners: HashMap<u32, MeshOwner>,
    installed_owners: HashMap<u32, (MeshOwner, Option<bevy::asset::AssetId<Image>>)>,
    blocked: HashSet<u32>,
    mesh_tick: Option<bevy::ecs::change_detection::Tick>,
    image_tick: Option<bevy::ecs::change_detection::Tick>,
    image_conflicts: HashSet<u32>,
    display_bound: HashMap<u32, bool>,
}

impl Default for MeshPaintingResource {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshPaintingResource {
    pub(crate) fn restore_uv_surface(&mut self, surface: MeshUvSurface) {
        self.uv_surfaces.insert(surface.mesh_id, surface);
    }
    /// Create a new mesh painting resource.
    pub fn new() -> Self {
        Self {
            uv_surfaces: HashMap::new(),
            uv_layers: HashMap::new(),
            shared_display_bytes: HashMap::new(),
            shared_original_bytes: HashMap::new(),
            ptex_surfaces: HashMap::new(),
            brush_color: [0.0, 0.0, 0.0, 1.0],
            brush_preset: BrushPreset::default(),
            blend_mode: BlendMode::Normal,
            active_stroke: None,
            projection_preview: None,
            history: UvHistory::default(),
            owners: HashMap::new(),
            installed_owners: HashMap::new(),
            blocked: HashSet::new(),
            mesh_tick: None,
            image_tick: None,
            image_conflicts: HashSet::new(),
            display_bound: HashMap::new(),
        }
    }

    /// Get or create a UV surface for a mesh.
    pub fn get_or_create_uv_surface(
        &mut self,
        mesh_id: u32,
        width: u32,
        height: u32,
    ) -> &mut MeshUvSurface {
        self.invalidate_mesh(mesh_id);
        self.uv_surfaces
            .entry(mesh_id)
            .or_insert_with(|| MeshUvSurface::new(mesh_id, width, height, 2))
    }

    /// Get or create a Ptex surface for a mesh.
    pub fn get_or_create_ptex_surface(
        &mut self,
        mesh_id: u32,
        face_resolution: u32,
    ) -> &mut MeshPtexSurface {
        self.ptex_surfaces
            .entry(mesh_id)
            .or_insert_with(|| MeshPtexSurface::new(mesh_id, face_resolution))
    }

    /// Get a UV surface by mesh_id.
    pub fn get_uv_surface(&self, mesh_id: u32) -> Option<&MeshUvSurface> {
        self.uv_surfaces.get(&mesh_id)
    }

    /// Get a mutable UV surface by mesh_id.
    pub fn get_uv_surface_mut(&mut self, mesh_id: u32) -> Option<&mut MeshUvSurface> {
        self.invalidate_mesh(mesh_id);
        self.uv_surfaces.get_mut(&mesh_id)
    }
    fn invalidate_mesh(&mut self, id: u32) {
        if self
            .projection_preview
            .as_ref()
            .is_some_and(|p| p.mesh_id == id)
        {
            self.projection_preview = None;
            self.image_conflicts.insert(id);
            self.blocked.insert(id);
        }
        if let Some(layers) = self.uv_layers.get_mut(&id) {
            layers.clear_history();
        }
        self.history.clear_mesh(id);
        if self.active_stroke.as_ref().is_some_and(|s| s.mesh_id == id) {
            self.active_stroke = None;
        }
        self.display_bound.remove(&id);
    }

    pub(crate) fn record_installed_owner(
        &mut self,
        id: u32,
        entity: Entity,
        mesh: bevy::asset::AssetId<Mesh>,
        storage: MeshStorageMode,
        image: bevy::asset::AssetId<Image>,
        original: Option<bevy::asset::AssetId<Image>>,
    ) {
        self.installed_owners.entry(id).or_insert((
            MeshOwner {
                entity,
                mesh,
                storage,
                image,
            },
            original,
        ));
    }
    pub(crate) fn verified_installed_owner(
        &self,
        id: u32,
        entity: Entity,
        mesh: bevy::asset::AssetId<Mesh>,
        storage: MeshStorageMode,
        image: bevy::asset::AssetId<Image>,
        original: Option<bevy::asset::AssetId<Image>>,
    ) -> bool {
        self.installed_owners.get(&id)
            == Some(&(
                MeshOwner {
                    entity,
                    mesh,
                    storage,
                    image,
                },
                original,
            ))
    }
    pub(crate) fn accept_layer_migration(&mut self, id: u32) {
        self.image_conflicts.remove(&id);
        self.blocked.remove(&id);
        self.history.clear_mesh(id);
    }
    pub fn uv_layers(&self, id: u32) -> Option<&painting::uv_layers::UvLayers> {
        self.uv_layers.get(&id)
    }
    fn discard_uv_redo(&mut self) {
        self.history.redo.clear();
        for layers in self.uv_layers.values_mut() {
            layers.clear_redo();
        }
    }
    fn trim_shared_history(&mut self, keep: u32) {
        while self.history_bytes() > crate::mesh_uv_history::MAX_HISTORY_BYTES
            || self.history.undo.len()
                + self.history.redo.len()
                + self
                    .uv_layers
                    .values()
                    .map(|l| l.undo_count() + l.redo_count())
                    .sum::<usize>()
                > 128
        {
            if self.history.undo.pop_front().is_some() || self.history.redo.pop_front().is_some() {
                self.history.evicted_strokes += 1;
                continue;
            }
            let other = self.uv_layers.iter().find_map(|(&id, l)| {
                (id != keep && l.undo_count() + l.redo_count() > 0).then_some(id)
            });
            let id = other.unwrap_or(keep);
            if !self
                .uv_layers
                .get_mut(&id)
                .is_some_and(|l| l.evict_oldest())
            {
                break;
            }
        }
    }
    pub(crate) fn sync_active_layer(&mut self, id: u32) {
        let Some(layers) = self.uv_layers.get(&id) else {
            return;
        };
        let surface = self.uv_surfaces.get_mut(&id).unwrap();
        surface
            .atlas
            .surface_mut()
            .pixels_mut()
            .copy_from_slice(&layers.active().pixels);
        let (w, h) = surface.dimensions();
        surface.atlas.mark_region_dirty(0, 0, w, h);
    }
    pub(crate) fn edit_uv_layer(
        &mut self,
        id: u32,
        op: painting::uv_layers::UvLayerOp,
    ) -> Result<bool, String> {
        if self.has_active_stroke() || self.has_projection_preview() || self.blocked.contains(&id) {
            return Err("Finish the UV transaction or reopen its conflicted owner".into());
        }
        let authoring = !matches!(&op, painting::uv_layers::UvLayerOp::Select(_));
        let changed = self
            .uv_layers
            .get_mut(&id)
            .ok_or("Enable UV layers first")?
            .edit(op)?;
        if changed && authoring {
            self.discard_uv_redo();
        }
        if changed {
            self.trim_shared_history(id);
            self.sync_active_layer(id);
        }
        Ok(changed)
    }
    pub(crate) fn project_uv_layer(
        &mut self,
        id: u32,
        pixels: &[[f32; 4]],
    ) -> Result<bool, String> {
        if self.has_active_stroke() || self.has_projection_preview() || self.blocked.contains(&id) {
            return Err("UV projection target is active or conflicted".into());
        }
        let changed = self
            .uv_layers
            .get_mut(&id)
            .ok_or("Enable UV layers first")?
            .project(pixels)?;
        if changed {
            self.discard_uv_redo();
        }
        if changed {
            self.trim_shared_history(id);
            self.sync_active_layer(id);
        }
        Ok(changed)
    }
    pub(crate) fn has_projection_preview(&self) -> bool {
        self.projection_preview.is_some()
    }
    pub(crate) fn preview_owned(&self, id: u32) -> bool {
        self.projection_preview
            .as_ref()
            .is_some_and(|p| p.mesh_id == id)
            && self.preview_valid()
    }
    pub(crate) fn preview_valid(&self) -> bool {
        self.projection_preview.as_ref().is_some_and(|p| {
            !self.blocked.contains(&p.mesh_id)
                && self
                    .uv_layers
                    .get(&p.mesh_id)
                    .is_some_and(|l| l.document().active_layer == p.layer && l.paintable().is_ok())
                && self.uv_surfaces.get(&p.mesh_id).is_some_and(|s| {
                    painting::uv_layers::same_uv_pixels(s.atlas.surface().pixels(), &p.pixels)
                })
        })
    }
    pub(crate) fn begin_projection_preview(&mut self, id: u32) -> Result<(), String> {
        if self.has_active_stroke() || self.has_projection_preview() || self.blocked.contains(&id) {
            return Err("UV preview target is active or conflicted".into());
        }
        let layers = self.uv_layers.get(&id).ok_or("Enable UV layers first")?;
        layers.paintable()?;
        if layers.active().pixels.len() * 16 > painting::uv_layers::UV_PENDING_BYTES {
            return Err("UV preview exceeds the 32 MiB pending payload limit".into());
        }
        self.projection_preview = Some(UvProjectionPreview {
            mesh_id: id,
            layer: layers.document().active_layer,
            pixels: layers.active().pixels.clone(),
        });
        Ok(())
    }
    pub(crate) fn update_projection_preview(
        &mut self,
        id: u32,
        source: &[[f32; 4]],
    ) -> Result<(), String> {
        if !self.preview_valid() {
            return Err("UV preview owner changed outside its transaction".into());
        }
        let layers = self.uv_layers.get(&id).ok_or("Missing UV preview layer")?;
        if !painting::uv_layers::valid_uv_pixels(source, layers.active().pixels.len()) {
            return Err("Invalid UV projection preview pixels".into());
        }
        let preview = self.projection_preview.as_mut().unwrap();
        if preview.mesh_id != id {
            return Err("UV preview receiver changed".into());
        }
        for ((out, src), dst) in preview
            .pixels
            .iter_mut()
            .zip(source)
            .zip(&layers.active().pixels)
        {
            *out = painting::uv_layers::uv_over(*src, *dst);
        }
        let surface = self.uv_surfaces.get_mut(&id).unwrap();
        surface
            .atlas
            .surface_mut()
            .pixels_mut()
            .copy_from_slice(&preview.pixels);
        let (w, h) = surface.dimensions();
        surface.atlas.mark_region_dirty(0, 0, w, h);
        Ok(())
    }
    pub(crate) fn cancel_projection_preview(&mut self) {
        // Restore our workspace only while its exact bytes are still owned.
        let valid = self.preview_valid();
        if let Some(p) = self.projection_preview.take() {
            if valid {
                self.sync_active_layer(p.mesh_id);
            }
        }
    }
    pub(crate) fn commit_projection_preview(&mut self) -> Result<bool, String> {
        if !self.preview_valid() {
            return Err("UV preview target is conflicted".into());
        }
        let p = self.projection_preview.take().unwrap();
        let result = self
            .uv_layers
            .get_mut(&p.mesh_id)
            .unwrap()
            .paint(p.layer, p.pixels);
        if result.as_ref().is_ok_and(|v| *v) {
            self.discard_uv_redo();
            self.trim_shared_history(p.mesh_id);
        }
        self.sync_active_layer(p.mesh_id);
        result
    }
    pub(crate) fn shared_id_for_entity(&self, entity: Entity) -> Option<u32> {
        self.owners.iter().find_map(|(&id, o)| {
            (o.entity == entity && self.uv_layers.contains_key(&id)).then_some(id)
        })
    }

    /// Get a Ptex surface by mesh_id.
    pub fn get_ptex_surface(&self, mesh_id: u32) -> Option<&MeshPtexSurface> {
        self.ptex_surfaces.get(&mesh_id)
    }

    /// Get a mutable Ptex surface by mesh_id.
    pub fn get_ptex_surface_mut(&mut self, mesh_id: u32) -> Option<&mut MeshPtexSurface> {
        self.ptex_surfaces.get_mut(&mesh_id)
    }

    /// Set brush color.
    pub fn set_brush_color(&mut self, color: [f32; 4]) {
        self.brush_color = color;
    }

    /// Set brush preset.
    pub fn set_brush_preset(&mut self, preset: BrushPreset) {
        self.brush_preset = preset;
    }

    /// Set blend mode.
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        self.blend_mode = mode;
    }
    pub fn has_active_stroke(&self) -> bool {
        self.active_stroke.is_some()
    }
    /// Actual retained pixel snapshot bytes; live authoring surfaces are separate.
    pub fn history_bytes(&self) -> usize {
        self.history.bytes()
            + self
                .uv_layers
                .values()
                .map(|l| l.history_bytes())
                .sum::<usize>()
    }
    pub fn history_limit_bytes(&self) -> usize {
        crate::mesh_uv_history::MAX_HISTORY_BYTES
    }
    pub fn history_conflicted(&self, id: u32) -> bool {
        self.blocked.contains(&id)
    }
    pub fn pending_history_bytes(&self) -> usize {
        self.active_stroke
            .as_ref()
            .and_then(|s| s.before.as_ref())
            .map_or(0, |p| p.len() * 16)
            + self
                .projection_preview
                .as_ref()
                .map_or(0, |p| p.pixels.len() * 16)
    }
    pub fn evicted_history_strokes(&self) -> usize {
        self.history.evicted_strokes + self.uv_layers.values().map(|l| l.evicted()).sum::<usize>()
    }
    pub fn undo_count(&self, id: u32) -> usize {
        self.uv_layers.get(&id).map_or_else(
            || self.history.undo.iter().filter(|e| e.mesh_id == id).count(),
            |l| l.undo_count(),
        )
    }
    pub fn redo_count(&self, id: u32) -> usize {
        self.uv_layers.get(&id).map_or_else(
            || self.history.redo.iter().filter(|e| e.mesh_id == id).count(),
            |l| l.redo_count(),
        )
    }
    fn undo(&mut self, id: u32) -> bool {
        self.exchange(id, false)
    }
    fn redo(&mut self, id: u32) -> bool {
        self.exchange(id, true)
    }
    fn exchange(&mut self, id: u32, redo: bool) -> bool {
        if self.has_active_stroke() || self.has_projection_preview() {
            return false;
        }
        if let Some(layers) = self.uv_layers.get_mut(&id) {
            let changed = layers.exchange(redo);
            if changed {
                self.sync_active_layer(id);
            }
            return changed;
        }
        let stack = if redo {
            &self.history.redo
        } else {
            &self.history.undo
        };
        let Some(index) = stack.iter().rposition(|e| e.mesh_id == id) else {
            return false;
        };
        let entry = &stack[index];
        let Some(surface) = self.uv_surfaces.get(&id) else {
            return false;
        };
        if surface.dimensions() != entry.dimensions
            || !same_pixels(
                surface.atlas.surface().pixels(),
                if redo { &entry.before } else { &entry.after },
            )
        {
            self.history.clear_mesh(id);
            return false;
        }
        let entry = if redo {
            self.history.redo.remove(index)
        } else {
            self.history.undo.remove(index)
        }
        .unwrap();
        let surface = self.uv_surfaces.get_mut(&id).unwrap();
        surface
            .atlas
            .surface_mut()
            .pixels_mut()
            .copy_from_slice(if redo { &entry.after } else { &entry.before });
        surface
            .atlas
            .mark_region_dirty(0, 0, entry.dimensions.0, entry.dimensions.1);
        self.display_bound.insert(
            id,
            if redo {
                entry.after_bound
            } else {
                entry.before_bound
            },
        );
        if redo {
            self.history.undo.push_back(entry);
        } else {
            self.history.redo.push_back(entry);
        }
        true
    }
    fn begin(&mut self, entity: Entity, paintable: &PaintableMesh, before_bound: bool) -> bool {
        if self.has_active_stroke()
            || self.has_projection_preview()
            || self.blocked.contains(&paintable.mesh_id)
            || self
                .uv_layers
                .get(&paintable.mesh_id)
                .is_some_and(|l| l.paintable().is_err())
        {
            return false;
        }
        let (before, dimensions) = match paintable.storage_mode {
            MeshStorageMode::UvAtlas { resolution } => {
                let Some(surface) = self.uv_surfaces.get(&paintable.mesh_id) else {
                    return false;
                };
                if surface.dimensions() != resolution
                    || !surface
                        .atlas
                        .surface()
                        .pixels()
                        .iter()
                        .flatten()
                        .all(|p| p.is_finite() && (0.0..=1.0).contains(p))
                {
                    return false;
                }
                let count = surface.atlas.surface().pixels().len();
                if count.saturating_mul(32) > crate::mesh_uv_history::MAX_HISTORY_BYTES {
                    return false;
                }
                (Some(surface.atlas.surface().pixels().to_vec()), resolution)
            }
            MeshStorageMode::Ptex { .. } => (None, (0, 0)),
        };
        self.active_stroke = Some(ActiveMeshStroke {
            last_face: None,
            entity,
            mesh_id: paintable.mesh_id,
            before,
            dimensions,
            before_bound,
            layer: self
                .uv_layers
                .get(&paintable.mesh_id)
                .map(|l| l.document().active_layer),
            brush: painting::brush::BrushEngine::new(self.brush_preset.clone()),
            color: self.brush_color,
            blend: self.blend_mode,
        });
        true
    }
    fn finish(&mut self, cancel: bool) {
        let Some(active) = self.active_stroke.take() else {
            return;
        };
        let Some(before) = active.before else {
            return;
        };
        let Some(surface) = self.uv_surfaces.get_mut(&active.mesh_id) else {
            return;
        };
        if surface.dimensions() != active.dimensions {
            self.history.clear_mesh(active.mesh_id);
            return;
        }
        let valid = surface
            .atlas
            .surface()
            .pixels()
            .iter()
            .flatten()
            .all(|p| p.is_finite() && (0.0..=1.0).contains(p));
        if cancel || !valid {
            surface
                .atlas
                .surface_mut()
                .pixels_mut()
                .copy_from_slice(&before);
            surface
                .atlas
                .mark_region_dirty(0, 0, active.dimensions.0, active.dimensions.1);
            self.display_bound
                .insert(active.mesh_id, active.before_bound);
            return;
        }
        if same_pixels(&before, surface.atlas.surface().pixels()) {
            return;
        }
        if let Some(layer_id) = active.layer {
            let result = self
                .uv_layers
                .get_mut(&active.mesh_id)
                .unwrap()
                .paint(layer_id, surface.atlas.surface().pixels().to_vec());
            if result.is_err() {
                surface
                    .atlas
                    .surface_mut()
                    .pixels_mut()
                    .copy_from_slice(&before);
                surface
                    .atlas
                    .mark_region_dirty(0, 0, active.dimensions.0, active.dimensions.1);
            }
            if matches!(result, Ok(true)) {
                self.discard_uv_redo();
            }
            self.trim_shared_history(active.mesh_id);
            return;
        }
        let after = surface.atlas.surface().pixels().to_vec();
        self.discard_uv_redo();
        self.history.reserve(before.len() * 32);
        self.display_bound.insert(active.mesh_id, true);
        self.history.undo.push_back(UvEntry {
            mesh_id: active.mesh_id,
            dimensions: active.dimensions,
            before_bound: active.before_bound,
            after_bound: true,
            before,
            after,
        });
        self.trim_shared_history(active.mesh_id);
    }
}

/// Component linking a PaintableMesh to its GPU texture.
#[derive(Component, Clone)]
pub struct MeshPaintTexture {
    /// Handle to the Bevy Image asset (composited paint + original)
    pub image_handle: Handle<Image>,
    /// Handle to original material texture (if any) for compositing
    pub original_texture: Option<Handle<Image>>,
    /// Original base color from material (for meshes without texture)
    pub original_base_color: [f32; 4],
    pub(crate) original_color_source: Color,
    /// Whether this needs full upload
    pub needs_full_upload: bool,
    /// Whether any paint has been applied (don't touch material until painting)
    pub has_paint: bool,
}

/// Plugin for mesh painting system.
pub struct MeshPaintingSystemPlugin;

impl Plugin for MeshPaintingSystemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MeshPaintingResource>().add_systems(
            Update,
            (
                setup_mesh_paint_textures,
                sync_mesh_paint_owners,
                process_mesh_paint_events,
                upload_mesh_dirty_tiles,
            )
                .chain(),
        );
    }
}

/// History must be invoked through the world boundary so geometry/owner edits are
/// observed before any pixels are exchanged. Rendering settles on the next Update.
pub fn undo_mesh_paint(world: &mut World, entity: Entity) -> bool {
    exchange_mesh_paint(world, entity, false)
}
pub fn redo_mesh_paint(world: &mut World, entity: Entity) -> bool {
    exchange_mesh_paint(world, entity, true)
}
fn exchange_mesh_paint(world: &mut World, entity: Entity, redo: bool) -> bool {
    sync_mesh_paint_owners(world);
    if world.resource::<MeshPaintingResource>().has_active_stroke() {
        return false;
    }
    let Some(p) = world.get::<PaintableMesh>(entity) else {
        return false;
    };
    let id = p.mesh_id;
    let Some(handle) = world.get::<MeshMaterial3d<StandardMaterial>>(entity) else {
        return false;
    };
    let Some(material) = world.resource::<Assets<StandardMaterial>>().get(&handle.0) else {
        return false;
    };
    if crate::uv_layer_scene::validate_display(world, entity, material).is_err() {
        world
            .resource_mut::<MeshPaintingResource>()
            .invalidate_mesh(id);
        return false;
    }
    let mut r = world.resource_mut::<MeshPaintingResource>();
    if r.blocked.contains(&id) {
        return false;
    }
    if redo { r.redo(id) } else { r.undo(id) }
}

/// Conservative conflict admission: any mutation of mesh assets invalidates local
/// DirectUV history. Entity/storage/texture replacement and duplicate IDs also
/// invalidate it. This uses Bevy change ticks, with no geometry hash collision.
pub(crate) fn sync_mesh_paint_owners(world: &mut World) {
    let tick = world
        .get_resource_ref::<Assets<Mesh>>()
        .map(|r| r.last_changed());
    let image_tick = world
        .get_resource_ref::<Assets<Image>>()
        .map(|r| r.last_changed());
    let external_images = {
        let r = world.resource::<MeshPaintingResource>();
        r.image_tick.is_some() && image_tick != r.image_tick
    };
    let mut owners = HashMap::new();
    let mut blocked = HashSet::new();
    for (entity, p, mesh, texture, handle) in world
        .query::<(
            Entity,
            &PaintableMesh,
            &Mesh3d,
            &MeshPaintTexture,
            Option<&MeshMaterial3d<StandardMaterial>>,
        )>()
        .iter(world)
    {
        let valid = handle
            .and_then(|h| world.get_resource::<Assets<StandardMaterial>>()?.get(&h.0))
            .is_some_and(|m| {
                if texture.has_paint {
                    m.base_color == Color::WHITE
                        && m.base_color_texture.as_ref() == Some(&texture.image_handle)
                } else {
                    m.base_color == texture.original_color_source
                        && m.base_color_texture == texture.original_texture
                }
            });
        if !valid
            || !world
                .get_resource::<Assets<Mesh>>()
                .is_some_and(|m| m.contains(mesh.0.id()))
        {
            blocked.insert(p.mesh_id);
        }
        if external_images && matches!(p.storage_mode, MeshStorageMode::UvAtlas { .. }) {
            let r = world.resource::<MeshPaintingResource>();
            // A pending upload cannot distinguish an external replacement from
            // its expected old display, so conservatively refuse that conflict.
            let pending = r
                .get_uv_surface(p.mesh_id)
                .is_some_and(|s| s.has_dirty_tiles());
            let material =
                handle.and_then(|h| world.get_resource::<Assets<StandardMaterial>>()?.get(&h.0));
            if (pending && !r.uv_layers.contains_key(&p.mesh_id))
                || material.is_none_or(|m| {
                    crate::uv_layer_scene::validate_display(world, entity, m).is_err()
                })
            {
                blocked.insert(p.mesh_id);
            }
        }
        if owners
            .insert(
                p.mesh_id,
                MeshOwner {
                    entity,
                    mesh: mesh.0.id(),
                    storage: p.storage_mode,
                    image: texture.image_handle.id(),
                },
            )
            .is_some()
        {
            blocked.insert(p.mesh_id);
        }
    }
    let mut r = world.resource_mut::<MeshPaintingResource>();
    if external_images {
        r.image_conflicts.extend(blocked.iter().copied());
    }
    blocked.extend(r.image_conflicts.iter().copied());
    if r.mesh_tick.is_some() && tick != r.mesh_tick {
        r.history.undo.clear();
        r.history.redo.clear();
        for layers in r.uv_layers.values_mut() {
            layers.clear_history();
        }
        r.active_stroke = None;
        r.display_bound.clear();
    }
    let invalid: Vec<_> = r
        .owners
        .iter()
        .filter_map(|(id, old)| {
            (owners.get(id) != Some(old) || blocked.contains(id)).then_some(*id)
        })
        .collect();
    for id in invalid {
        r.invalidate_mesh(id);
    }
    r.owners = owners;
    r.blocked = blocked;
    r.mesh_tick = tick;
    r.image_tick = image_tick;
    // Keep edits following a synchronous history call observable even when
    // several world operations occur without an intervening App::update.
    world.increment_change_tick();
}

/// Set up paint textures for newly added PaintableMesh entities.
fn setup_mesh_paint_textures(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    materials: Res<Assets<StandardMaterial>>,
    mut painting_res: ResMut<MeshPaintingResource>,
    query: Query<
        (
            Entity,
            &PaintableMesh,
            Option<&MeshMaterial3d<StandardMaterial>>,
            Option<&Mesh3d>,
        ),
        Without<MeshPaintTexture>,
    >,
) {
    for (entity, paintable, material_handle, mesh) in query.iter() {
        let (width, height) = match paintable.storage_mode {
            MeshStorageMode::UvAtlas { resolution } => resolution,
            MeshStorageMode::Ptex { face_resolution } => {
                // For Ptex, we create a placeholder texture
                // Actual per-face textures are managed separately
                (face_resolution * 16, face_resolution * 16)
            }
        };

        // Create the paint texture image
        let mut image = Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0], // Transparent
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        );

        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT;

        let image_handle = images.add(image);

        // Initialize the surface
        match paintable.storage_mode {
            MeshStorageMode::UvAtlas { resolution } => {
                let surface = painting_res.get_or_create_uv_surface(
                    paintable.mesh_id,
                    resolution.0,
                    resolution.1,
                );
                let _ = surface; // Existing/restored authoring pixels retain their owner.
            }
            MeshStorageMode::Ptex { face_resolution } => {
                painting_res.get_or_create_ptex_surface(paintable.mesh_id, face_resolution);
            }
        }

        // Extract original texture and color from material (don't modify material yet)
        let (original_texture, original_base_color) = if let Some(material_ref) = material_handle {
            if let Some(material) = materials.get(&material_ref.0) {
                let color = material.base_color.to_linear();
                (
                    material.base_color_texture.clone(),
                    [color.red, color.green, color.blue, color.alpha],
                )
            } else {
                (None, [0.8, 0.8, 0.8, 1.0])
            }
        } else {
            (None, [0.8, 0.8, 0.8, 1.0])
        };

        painting_res.shared_display_bytes.insert(
            paintable.mesh_id,
            images
                .get(&image_handle)
                .and_then(|i| i.data.clone())
                .unwrap_or_default(),
        );
        painting_res.shared_original_bytes.insert(
            paintable.mesh_id,
            original_texture
                .as_ref()
                .and_then(|h| images.get(h))
                .and_then(|i| i.data.clone()),
        );
        if let Some(mesh) = mesh {
            painting_res.record_installed_owner(
                paintable.mesh_id,
                entity,
                mesh.0.id(),
                paintable.storage_mode,
                image_handle.id(),
                original_texture.as_ref().map(|h| h.id()),
            );
        }
        commands.entity(entity).insert(MeshPaintTexture {
            image_handle,
            original_texture,
            original_base_color,
            original_color_source: material_handle.and_then(|h| materials.get(&h.0)).map_or(
                Color::linear_rgba(
                    original_base_color[0],
                    original_base_color[1],
                    original_base_color[2],
                    original_base_color[3],
                ),
                |m| m.base_color,
            ),
            needs_full_upload: false, // Don't upload until we have paint
            has_paint: false,
        });

        info!(
            "Set up paint texture for mesh_id={} ({}x{})",
            paintable.mesh_id, width, height
        );
    }
}

/// Process mesh paint events and apply dabs to surfaces.
pub(crate) fn process_mesh_paint_events(
    mut mesh_paint_events: MessageReader<MeshPaintEvent>,
    mut painting_res: ResMut<MeshPaintingResource>,
    mesh_query: Query<(&PaintableMesh, &MeshPaintTexture)>,
    geometry: Query<&Mesh3d>,
    meshes: Res<Assets<Mesh>>,
) {
    for event in mesh_paint_events.read() {
        match event {
            MeshPaintEvent::StrokeStart {
                mesh_entity,
                mesh_id,
                hit,
                stroke_id,
            }
            | MeshPaintEvent::StrokeStartWithPressure {
                mesh_entity,
                mesh_id,
                hit,
                stroke_id,
                ..
            } => {
                let pressure = match event {
                    MeshPaintEvent::StrokeStartWithPressure { pressure, .. } => *pressure,
                    _ => 1.,
                };
                if !pressure.is_finite() || !(0.0..=1.0).contains(&pressure) {
                    continue;
                }
                info!(
                    "Mesh stroke start: mesh_id={}, stroke_id={}",
                    mesh_id, stroke_id
                );
                if let Ok((paintable, texture)) = mesh_query.get(*mesh_entity)
                    && paintable.mesh_id == *mesh_id
                    && painting_res.begin(*mesh_entity, paintable, texture.has_paint)
                {
                    apply_dab_to_mesh(&mut painting_res, &mesh_query, *mesh_entity, hit, pressure);
                    painting_res.active_stroke.as_mut().unwrap().last_face = Some(hit.face_id);
                }
            }
            MeshPaintEvent::StrokeMove {
                hit,
                pressure,
                speed: _,
            } => {
                if let Some(active) = painting_res.active_stroke.as_ref() {
                    let entity = active.entity;
                    let id = active.mesh_id;
                    if mesh_query.get(entity).is_ok_and(|(p, _)| p.mesh_id == id) {
                        let continuous = active.last_face.is_some_and(|previous| {
                            geometry
                                .get(entity)
                                .ok()
                                .and_then(|h| meshes.get(&h.0))
                                .is_some_and(|mesh| {
                                    uv_faces_continuous(mesh, previous, hit.face_id)
                                })
                        });
                        let active = painting_res.active_stroke.as_mut().unwrap();
                        if !continuous {
                            active.brush.begin_stroke();
                        }
                        active.last_face = Some(hit.face_id);
                        apply_dab_for_move(&mut painting_res, id, hit, *pressure);
                    } else {
                        painting_res.finish(true);
                    }
                }
            }
            MeshPaintEvent::StrokeEnd => {
                painting_res.finish(false);
                info!("Mesh stroke end");
            }
            MeshPaintEvent::StrokeCancel => {
                painting_res.finish(true);
                info!("Mesh stroke cancelled");
            }
            MeshPaintEvent::StrokeBreak => {
                if let Some(active) = painting_res.active_stroke.as_mut() {
                    active.brush.begin_stroke();
                    active.last_face = None;
                }
            }
            MeshPaintEvent::History { mesh_entity, redo } => {
                if let Ok((p, _)) = mesh_query.get(*mesh_entity) {
                    if !painting_res.blocked.contains(&p.mesh_id)
                        && matches!(p.storage_mode, MeshStorageMode::UvAtlas { .. })
                    {
                        let _ = painting_res.exchange(p.mesh_id, *redo);
                    }
                }
            }
        }
    }
}

/// Interpolation is admitted only within one face or across an exact shared UV edge.
/// Duplicated seam vertices may share positions; their UV endpoints must also match.
pub(crate) fn uv_faces_continuous(mesh: &Mesh, before: u32, after: u32) -> bool {
    if before == after {
        return true;
    }
    use bevy::mesh::{Indices, VertexAttributeValues};
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return false;
    };
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        return false;
    };
    let index = |n: usize| -> Option<usize> {
        match mesh.indices() {
            Some(Indices::U32(v)) => v.get(n).map(|i| *i as usize),
            Some(Indices::U16(v)) => v.get(n).map(|i| *i as usize),
            None => (n < positions.len()).then_some(n),
        }
    };
    let vertices = |face: u32| -> Option<Vec<([f32; 3], [f32; 2])>> {
        (0..3)
            .map(|offset| {
                let i = index(face as usize * 3 + offset)?;
                Some((*positions.get(i)?, *uvs.get(i)?))
            })
            .collect()
    };
    let (Some(a), Some(b)) = (vertices(before), vertices(after)) else {
        return false;
    };
    a.iter().filter(|v| b.contains(v)).count() >= 2
}

/// Apply a dab to a mesh surface based on hit data.
fn apply_dab_to_mesh(
    painting_res: &mut MeshPaintingResource,
    mesh_query: &Query<(&PaintableMesh, &MeshPaintTexture)>,
    mesh_entity: Entity,
    hit: &MeshHit,
    pressure: f32,
) {
    let Ok((paintable, _)) = mesh_query.get(mesh_entity) else {
        return;
    };

    if hit
        .uv
        .is_some_and(|uv| !uv.is_finite() || uv.min_element() < 0.0 || uv.max_element() > 1.0)
    {
        return;
    }
    let brush_size = painting_res.brush_preset.base_size;
    let color = painting_res.brush_color;
    let opacity = painting_res.brush_preset.opacity;
    let hardness = painting_res.brush_preset.hardness;
    let blend_mode = painting_res.blend_mode;

    match paintable.storage_mode {
        MeshStorageMode::UvAtlas { .. } => {
            apply_uv_input(painting_res, paintable.mesh_id, hit, pressure);
        }
        MeshStorageMode::Ptex { face_resolution } => {
            let surface =
                painting_res.get_or_create_ptex_surface(paintable.mesh_id, face_resolution);

            // Convert barycentric to face-local coordinates
            let local_coords = Vec2::new(
                hit.barycentric.x * face_resolution as f32,
                hit.barycentric.y * face_resolution as f32,
            );

            surface.apply_dab(
                hit.face_id,
                local_coords,
                brush_size,
                color,
                opacity,
                hardness,
                blend_mode,
            );
        }
    }
}

/// Apply a dab for stroke move event.
fn apply_dab_for_move(
    painting_res: &mut MeshPaintingResource,
    mesh_id: u32,
    hit: &MeshHit,
    pressure: f32,
) {
    if !pressure.is_finite()
        || !(0.0..=1.0).contains(&pressure)
        || hit
            .uv
            .is_some_and(|uv| !uv.is_finite() || uv.min_element() < 0.0 || uv.max_element() > 1.0)
    {
        return;
    }
    let brush_size = painting_res.brush_preset.size_for_pressure(pressure);
    let color = painting_res.brush_color;
    let opacity = painting_res.brush_preset.opacity;
    let hardness = painting_res.brush_preset.hardness;
    let blend_mode = painting_res.blend_mode;

    if painting_res.uv_surfaces.contains_key(&mesh_id) {
        apply_uv_input(painting_res, mesh_id, hit, pressure);
        return;
    }
    // Try Ptex surface
    if let Some(surface) = painting_res.get_ptex_surface_mut(mesh_id) {
        let face_resolution = surface.default_resolution;
        let local_coords = Vec2::new(
            hit.barycentric.x * face_resolution as f32,
            hit.barycentric.y * face_resolution as f32,
        );

        surface.apply_dab(
            hit.face_id,
            local_coords,
            brush_size,
            color,
            opacity,
            hardness,
            blend_mode,
        );
    }
}

fn apply_uv_input(r: &mut MeshPaintingResource, id: u32, hit: &MeshHit, pressure: f32) {
    let Some(uv) = hit
        .uv
        .filter(|uv| uv.is_finite() && uv.min_element() >= 0.0 && uv.max_element() <= 1.0)
    else {
        return;
    };
    let Some(active) = r.active_stroke.as_mut().filter(|a| a.mesh_id == id) else {
        return;
    };
    let (w, h) = active.dimensions;
    let dabs = active
        .brush
        .stroke_to(uv.x * w as f32, (1.0 - uv.y) * h as f32, pressure);
    let color = active.color;
    let blend = active.blend;
    let Some(surface) = r.uv_surfaces.get_mut(&id) else {
        return;
    };
    for dab in dabs {
        let apply = if r.uv_layers.contains_key(&id) {
            painting::tiles::TiledSurface::apply_dab_ellipse_uv
        } else {
            painting::tiles::TiledSurface::apply_dab_ellipse
        };
        apply(
            &mut surface.atlas,
            dab.x,
            dab.y,
            dab.size / 2.0,
            color,
            dab.opacity,
            dab.hardness,
            blend,
            0.0,
            1.0,
        );
    }
}

/// Upload dirty tiles to GPU for UV surfaces, compositing paint over original texture.
fn upload_mesh_dirty_tiles(
    mut painting_res: ResMut<MeshPaintingResource>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut query: Query<(
        &PaintableMesh,
        &mut MeshPaintTexture,
        Option<&MeshMaterial3d<StandardMaterial>>,
    )>,
) {
    for (paintable, mut paint_texture, material_handle) in query.iter_mut() {
        if painting_res.blocked.contains(&paintable.mesh_id) {
            continue;
        }
        match paintable.storage_mode {
            MeshStorageMode::UvAtlas { .. } => {
                if let Some(surface) = painting_res.get_uv_surface(paintable.mesh_id) {
                    if !surface.has_dirty_tiles() && !paint_texture.needs_full_upload {
                        continue;
                    }

                    let original_data =
                        paint_texture.original_texture.as_ref().and_then(|handle| {
                            images.get(handle).and_then(|image| image.data.as_deref())
                        });
                    let composite = painting_res
                        .uv_layers
                        .get(&paintable.mesh_id)
                        .map(|layers| {
                            layers
                                .document()
                                .composite_active(Some(surface.atlas.surface().pixels()))
                        });
                    let data = if let Some(pixels) = &composite {
                        crate::uv_layer_scene::display_pixels(
                            pixels,
                            paint_texture.original_base_color,
                            original_data,
                        )
                    } else {
                        composite_uv(surface, paint_texture.original_base_color, original_data)
                    };
                    let bound = composite.as_ref().map_or_else(
                        || {
                            painting_res
                                .display_bound
                                .get(&paintable.mesh_id)
                                .copied()
                                .unwrap_or(true)
                        },
                        |pixels| pixels.iter().any(|p| p[3] > 0.),
                    );
                    if let Some(image) = images.get_mut(&paint_texture.image_handle) {
                        painting_res
                            .shared_display_bytes
                            .insert(paintable.mesh_id, data.clone());
                        image.data = Some(data);
                    }
                    if let Some(material_ref) = material_handle {
                        if let Some(material) = materials.get_mut(&material_ref.0) {
                            if bound {
                                material.base_color_texture =
                                    Some(paint_texture.image_handle.clone());
                                material.base_color = Color::WHITE;
                            } else {
                                material.base_color_texture =
                                    paint_texture.original_texture.clone();
                                material.base_color = paint_texture.original_color_source;
                            }
                        }
                    }
                    paint_texture.has_paint = bound;
                    painting_res.display_bound.remove(&paintable.mesh_id);

                    paint_texture.needs_full_upload = false;
                    painting_res
                        .uv_surfaces
                        .get_mut(&paintable.mesh_id)
                        .unwrap()
                        .atlas
                        .take_dirty_tiles();
                }
            }
            MeshStorageMode::Ptex { .. } => {
                // Ptex upload would require a different texture format
                // or compositing faces into an atlas
                // For now, this is a placeholder
            }
        }
    }
    painting_res.image_tick = Some(images.last_changed());
}

pub(crate) fn composite_uv(
    surface: &MeshUvSurface,
    original_base_color: [f32; 4],
    original_data: Option<&[u8]>,
) -> Vec<u8> {
    let cpu_surface = surface.surface().surface();
    let width = cpu_surface.width as usize;
    let height = cpu_surface.height as usize;

    // Composite paint over original
    let mut data = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;

            // Get original pixel (from texture or base color)
            let (orig_r, orig_g, orig_b, orig_a) = if let Some(orig) = original_data {
                if idx + 3 < orig.len() {
                    (orig[idx], orig[idx + 1], orig[idx + 2], orig[idx + 3])
                } else {
                    color_to_srgb_u8(original_base_color)
                }
            } else {
                color_to_srgb_u8(original_base_color)
            };

            // Get paint pixel
            if let Some(paint_pixel) = cpu_surface.get_pixel(x as u32, y as u32) {
                let paint_alpha = paint_pixel[3];

                if paint_alpha > 0.001 {
                    // Alpha blend paint over original
                    let paint_r = linear_to_srgb_u8(paint_pixel[0]);
                    let paint_g = linear_to_srgb_u8(paint_pixel[1]);
                    let paint_b = linear_to_srgb_u8(paint_pixel[2]);
                    let paint_a = (paint_alpha * 255.0) as u8;

                    let alpha = paint_a as f32 / 255.0;
                    let inv_alpha = 1.0 - alpha;

                    data[idx] = (paint_r as f32 * alpha + orig_r as f32 * inv_alpha) as u8;
                    data[idx + 1] = (paint_g as f32 * alpha + orig_g as f32 * inv_alpha) as u8;
                    data[idx + 2] = (paint_b as f32 * alpha + orig_b as f32 * inv_alpha) as u8;
                    data[idx + 3] = orig_a.max(paint_a);
                } else {
                    // No paint, use original
                    data[idx] = orig_r;
                    data[idx + 1] = orig_g;
                    data[idx + 2] = orig_b;
                    data[idx + 3] = orig_a;
                }
            } else {
                // No paint data, use original
                data[idx] = orig_r;
                data[idx + 1] = orig_g;
                data[idx + 2] = orig_b;
                data[idx + 3] = orig_a;
            }
        }
    }

    data
}

/// Convert linear [f32; 4] color to sRGB (u8, u8, u8, u8).
fn color_to_srgb_u8(color: [f32; 4]) -> (u8, u8, u8, u8) {
    (
        linear_to_srgb_u8(color[0]),
        linear_to_srgb_u8(color[1]),
        linear_to_srgb_u8(color[2]),
        (color[3] * 255.0) as u8,
    )
}

/// Convert linear color to sRGB u8.
fn linear_to_srgb_u8(linear: f32) -> u8 {
    let srgb = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0) as u8
}

#[cfg(test)]
mod mixed_history_regression_tests {
    use super::*;
    use painting::uv_layers::{UvLayerOp, UvLayers};

    fn legacy_commit(r: &mut MeshPaintingResource, size: u32, value: f32) {
        let p = PaintableMesh {
            mesh_id: 44,
            storage_mode: MeshStorageMode::UvAtlas {
                resolution: (size, size),
            },
        };
        assert!(r.begin(Entity::PLACEHOLDER, &p, false));
        r.uv_surfaces
            .get_mut(&44)
            .unwrap()
            .atlas
            .surface_mut()
            .pixels_mut()[0] = [value, 0., 0., 1.];
        r.finish(false);
        assert!(!r.has_active_stroke());
        assert_eq!(
            r.uv_surfaces[&44].atlas.surface().pixels()[0],
            [value, 0., 0., 1.],
            "history eviction must retain committed pixels"
        );
    }

    #[test]
    fn legacy_commit_enforces_combined_shared_history_entry_limit() {
        for shared_entries in [127, 128] {
            let mut r = MeshPaintingResource::default();
            r.get_or_create_uv_surface(44, 2, 2);
            let mut shared = UvLayers::new(1, 1).unwrap();
            for n in 0..shared_entries {
                shared
                    .edit(UvLayerOp::Rename(0, format!("Shared {n}")))
                    .unwrap();
            }
            r.uv_layers.insert(12, shared);
            legacy_commit(&mut r, 2, 0.2);
            assert_eq!(
                r.undo_count(12),
                shared_entries,
                "documented legacy-first eviction preserves shared entries"
            );
            assert_eq!(r.undo_count(44), usize::from(shared_entries == 127));
            assert_eq!(r.undo_count(12) + r.undo_count(44), 128);
            assert!(r.history_bytes() <= r.history_limit_bytes());
            let evicted = r.evicted_history_strokes();
            assert_eq!(evicted, usize::from(shared_entries == 128));
            let p = PaintableMesh {
                mesh_id: 44,
                storage_mode: MeshStorageMode::UvAtlas { resolution: (2, 2) },
            };
            assert!(r.begin(Entity::PLACEHOLDER, &p, true));
            r.finish(false); // no-op
            assert!(r.begin(Entity::PLACEHOLDER, &p, true));
            r.uv_surfaces
                .get_mut(&44)
                .unwrap()
                .atlas
                .surface_mut()
                .pixels_mut()[0] = [0.4, 0., 0., 1.];
            r.finish(true); // cancellation
            assert_eq!(r.evicted_history_strokes(), evicted);
            assert_eq!(r.undo_count(12) + r.undo_count(44), 128);
            assert_eq!(
                r.uv_surfaces[&44].atlas.surface().pixels()[0],
                [0.2, 0., 0., 1.]
            );
        }
    }

    #[test]
    fn legacy_commit_enforces_combined_shared_history_byte_limit() {
        let mut r = MeshPaintingResource::default();
        r.get_or_create_uv_surface(44, 512, 512);
        let mut shared = UvLayers::new(512, 512).unwrap();
        for n in 1..=7 {
            shared
                .paint(0, vec![[n as f32 / 10., 0., 0., 1.]; 512 * 512])
                .unwrap();
        }
        let shared_before = shared.document().clone();
        let retained = shared.history_bytes();
        assert!(retained < r.history_limit_bytes());
        assert!(
            retained + 512 * 512 * 32 > r.history_limit_bytes(),
            "this actual legacy stroke must cross the combined payload cap"
        );
        r.uv_layers.insert(12, shared);
        legacy_commit(&mut r, 512, 0.2);
        assert!(r.history_bytes() <= r.history_limit_bytes());
        assert_eq!(r.history_bytes(), retained);
        assert_eq!(r.undo_count(12), 7);
        assert_eq!(
            r.undo_count(44),
            0,
            "legacy-first policy can immediately expire the new entry"
        );
        assert_eq!(r.evicted_history_strokes(), 1);
        assert_eq!(r.uv_layers[&12].document(), &shared_before);
    }
}
