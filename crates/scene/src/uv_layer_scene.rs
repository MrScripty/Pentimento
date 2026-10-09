//! One shared UV appearance owner. Legacy projection never binds layered receivers.
//! Canvas Apply and DirectUV edit the same selected linear associated source.
use crate::{MeshPaintTexture, MeshPaintingResource, PaintMode, PaintableMesh};
use bevy::prelude::*;
use painting::uv_layers::{
    UV_COMPOSITOR, UvBlendMode, UvLayerDocument, UvLayerMeta, UvLayerOp, UvLayers,
    UvLayersDocument, UvPaintTarget,
};
use pentimento_ipc::{
    PaintCommand, UvLayerBlendMode as WireBlend, UvLayerCommand as Command, UvLayerInfo,
    UvLayerPaintTarget as WireTarget, UvLayerState, UvReceiverInfo,
};

pub(crate) fn composite_display(
    document: &UvLayersDocument,
    color: [f32; 4],
    original: Option<&[u8]>,
) -> Vec<u8> {
    let pixels = document.composite();
    display_pixels(&pixels, color, original)
}
pub(crate) fn display_pixels(
    pixels: &[[f32; 4]],
    color: [f32; 4],
    original: Option<&[u8]>,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(pixels.len() * 4);
    for (i, p) in pixels.iter().enumerate() {
        let mut base = color;
        if let Some(raw) = original.and_then(|v| v.get(i * 4..i * 4 + 4)) {
            for c in 0..3 {
                base[c] *= srgb_to_linear(raw[c]);
            }
            base[3] *= raw[3] as f32 / 255.;
        }
        let base_pm = [
            base[0] * base[3],
            base[1] * base[3],
            base[2] * base[3],
            base[3],
        ];
        let out = painting::uv_layers::uv_over(*p, base_pm);
        let rgb = if out[3] > 0. {
            [out[0] / out[3], out[1] / out[3], out[2] / out[3]]
        } else {
            [0.; 3]
        };
        bytes.extend(rgb.map(linear_to_srgb));
        bytes.push((out[3].clamp(0., 1.) * 255.).round() as u8);
    }
    bytes
}
fn srgb_to_linear(v: u8) -> f32 {
    let v = v as f32 / 255.;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_srgb(v: f32) -> u8 {
    let v = v.clamp(0., 1.);
    let s = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (s * 255.).round() as u8
}

#[cfg(test)]
mod blend_display_tests {
    use super::*;
    #[test]
    fn lone_blended_layer_keeps_isolated_stack_over_colored_original() {
        let mut d = UvLayers::new(2, 2).unwrap();
        d.paint(0, vec![[0.08, 0.28, 0.16, 0.4]; 4]).unwrap();
        let color = [0.2, 0.4, 0.7, 0.8];
        let texture = [20, 128, 200, 175].repeat(4);
        let normal = composite_display(d.document(), color, Some(&texture));
        for mode in [
            UvBlendMode::Multiply,
            UvBlendMode::Screen,
            UvBlendMode::Overlay,
        ] {
            d.edit(UvLayerOp::BlendMode(0, mode)).unwrap();
            assert_eq!(
                composite_display(d.document(), color, Some(&texture)),
                normal
            );
        }
        d.edit(UvLayerOp::Create("upper".into())).unwrap();
        let id = d.document().active_layer;
        d.paint(id, vec![[0.48, 0.18, 0.3, 0.6]; 4]).unwrap();
        let under_normal = composite_display(d.document(), color, Some(&texture));
        d.edit(UvLayerOp::BlendMode(id, UvBlendMode::Screen))
            .unwrap();
        assert_ne!(
            composite_display(d.document(), color, Some(&texture)),
            under_normal
        );
    }
}

pub(crate) fn validate_display(
    world: &World,
    entity: Entity,
    current: &StandardMaterial,
) -> Result<(), String> {
    let Some(p) = world.get::<PaintableMesh>(entity) else {
        return Ok(());
    };
    let Some(r) = world.get_resource::<MeshPaintingResource>() else {
        return Ok(());
    };
    let Some(layers) = r.uv_layers(p.mesh_id) else {
        return crate::project_uv::capture(world, entity, current).map(|_| ());
    };
    let t = world
        .get::<MeshPaintTexture>(entity)
        .ok_or("Missing shared UV image owner")?;
    let images = world
        .get_resource::<Assets<Image>>()
        .ok_or("Missing UV image assets")?;
    let display = images
        .get(&t.image_handle)
        .ok_or("Missing shared UV display")?;
    crate::project_assets::ImageDocument::capture(display)?
        .validate_direct_uv(layers.document().width, layers.document().height)?;
    if display.data.as_ref() != r.shared_display_bytes.get(&p.mesh_id) {
        return Err("Shared UV display bytes changed outside its owner".into());
    }
    if t.has_paint {
        if current.base_color != Color::WHITE
            || current.base_color_texture.as_ref() != Some(&t.image_handle)
        {
            return Err("Shared UV material binding changed outside its owner".into());
        }
    } else if current.base_color != t.original_color_source
        || current.base_color_texture != t.original_texture
    {
        return Err("Shared UV original binding changed outside its owner".into());
    }
    let original = t
        .original_texture
        .as_ref()
        .map(|h| images.get(h).ok_or("Missing original UV image"))
        .transpose()?;
    if let Some(image) = original {
        crate::project_assets::ImageDocument::capture(image)?
            .validate_direct_uv(layers.document().width, layers.document().height)?;
    }
    if original.and_then(|i| i.data.as_ref())
        != r.shared_original_bytes
            .get(&p.mesh_id)
            .and_then(|v| v.as_ref())
    {
        return Err("Original UV image bytes changed outside its owner".into());
    }
    if !r.has_active_stroke()
        && !r.preview_owned(p.mesh_id)
        && r.get_uv_surface(p.mesh_id)
            .is_none_or(|s| !layers.active().working_matches(s.atlas.surface().pixels()))
    {
        return Err("Selected UV pixels changed outside their layer owner".into());
    }
    Ok(())
}
pub(crate) fn capture(
    world: &World,
    entity: Entity,
    current: &StandardMaterial,
) -> Result<Option<(UvLayersDocument, StandardMaterial)>, String> {
    let Some(p) = world.get::<PaintableMesh>(entity) else {
        return Ok(None);
    };
    let Some(layers) = world
        .get_resource::<MeshPaintingResource>()
        .and_then(|r| r.uv_layers(p.mesh_id))
    else {
        return Ok(None);
    };
    validate_display(world, entity, current)?;
    let t = world.get::<MeshPaintTexture>(entity).unwrap();
    let images = world.resource::<Assets<Image>>();
    let expected = composite_display(
        layers.document(),
        t.original_base_color,
        t.original_texture
            .as_ref()
            .and_then(|h| images.get(h))
            .and_then(|i| i.data.as_deref()),
    );
    if images.get(&t.image_handle).and_then(|i| i.data.as_deref()) != Some(expected.as_slice()) {
        return Err("Shared UV display is pending; wait for settlement before saving".into());
    }
    let mut original = current.clone();
    original.base_color = t.original_color_source;
    original.base_color_texture = t.original_texture.clone();
    Ok(Some((layers.document().clone(), original)))
}
fn selected(world: &World) -> Option<(Entity, u32)> {
    let e = world.get_resource::<PaintMode>()?.direct_target?;
    Some((e, world.get::<PaintableMesh>(e)?.mesh_id))
}
pub(crate) fn state(world: &mut World) -> UvLayerState {
    let mut result = UvLayerState::default();
    let receivers: Vec<_> = world
        .query::<(Entity, &PaintableMesh, Option<&Name>)>()
        .iter(world)
        .filter(|(_, p, _)| matches!(p.storage_mode, painting::MeshStorageMode::UvAtlas { .. }))
        .map(|(e, p, n)| {
            (
                e,
                p.mesh_id,
                n.map_or_else(
                    || format!("UV receiver {}", p.mesh_id),
                    |n| n.as_str().to_string(),
                ),
            )
        })
        .collect();
    let Some(r) = world.get_resource::<MeshPaintingResource>() else {
        return result;
    };
    result.receivers = receivers
        .into_iter()
        .map(|(_, id, name)| UvReceiverInfo {
            mesh_id: id,
            name,
            layered: r.uv_layers(id).is_some(),
        })
        .collect();
    result.receivers.sort_by_key(|r| r.mesh_id);
    if let Some((_, id)) = selected(world) {
        result.receiver = Some(id);
        result.conflicted = r.history_conflicted(id);
        if let Some(l) = r.uv_layers(id) {
            result.enabled = true;
            result.layers = l
                .document()
                .layers
                .iter()
                .rev()
                .map(|l| UvLayerInfo {
                    id: l.meta.id,
                    name: l.meta.name.clone(),
                    visible: l.meta.visible,
                    opacity: l.meta.opacity,
                    locked: l.meta.locked,
                    blend_mode: match l.meta.blend_mode {
                        UvBlendMode::Normal => WireBlend::Normal,
                        UvBlendMode::Multiply => WireBlend::Multiply,
                        UvBlendMode::Screen => WireBlend::Screen,
                        UvBlendMode::Overlay => WireBlend::Overlay,
                    },
                    has_mask: l.mask.is_some(),
                    mask_enabled: l.meta.mask_enabled,
                    paint_target: match l.meta.paint_target {
                        UvPaintTarget::Color => WireTarget::Color,
                        UvPaintTarget::Mask => WireTarget::Mask,
                    },
                    is_active: l.meta.id == l_document_active(r, id),
                })
                .collect();
            result.can_undo = l.undo_count() > 0;
            result.can_redo = l.redo_count() > 0;
        }
    }
    result.projection_preview = world
        .get_resource::<crate::projection_painting::LiveUvPreview>()
        .is_some_and(|p| p.active());
    result.active = result.projection_preview
        || crate::brush_presets::active(world)
        || world
            .get_resource::<crate::projection_painting::PendingUvApplies>()
            .is_some_and(|p| p.bytes() > 0);
    result.can_undo &= !result.active && !result.conflicted;
    result.can_redo &= !result.active && !result.conflicted;
    result.notice = world
        .get_resource::<PaintMode>()
        .and_then(|p| p.target_notice.clone());
    result
}
fn l_document_active(r: &MeshPaintingResource, id: u32) -> u32 {
    r.uv_layers(id).unwrap().document().active_layer
}

pub(crate) fn enable(world: &mut World, entity: Entity) -> Result<(), String> {
    let p = world
        .get::<PaintableMesh>(entity)
        .ok_or("Choose a UV receiver")?
        .clone();
    if world
        .resource::<MeshPaintingResource>()
        .uv_layers(p.mesh_id)
        .is_some()
    {
        return Ok(());
    }
    let painting::MeshStorageMode::UvAtlas { resolution: (w, h) } = p.storage_mode else {
        return Err("PTex UV layers are unavailable".into());
    };
    if world
        .query::<&PaintableMesh>()
        .iter(world)
        .filter(|mesh| mesh.mesh_id == p.mesh_id)
        .count()
        != 1
    {
        return Err("UV migration refuses duplicate receiver identities".into());
    }
    let mesh_handle = world
        .get::<Mesh3d>(entity)
        .ok_or("Missing UV mesh")?
        .0
        .clone();
    let texture = world
        .get::<MeshPaintTexture>(entity)
        .ok_or("Missing installed UV owner")?;
    if !world
        .resource::<MeshPaintingResource>()
        .verified_installed_owner(
            p.mesh_id,
            entity,
            mesh_handle.id(),
            p.storage_mode,
            texture.image_handle.id(),
            texture.original_texture.as_ref().map(|h| h.id()),
        )
    {
        return Err(
            "UV migration refuses replaced entity, mesh, storage or image identities".into(),
        );
    }
    let geometry = crate::project_assets::MeshDocument::capture(
        world
            .resource::<Assets<Mesh>>()
            .get(&mesh_handle)
            .ok_or("Missing UV mesh asset")?,
    )?;
    geometry.validate()?;
    if !geometry.has_uv0() {
        return Err("UV migration requires UV0 geometry".into());
    }
    let current_handle = world
        .get::<MeshMaterial3d<StandardMaterial>>(entity)
        .ok_or("Missing UV material")?
        .0
        .clone();
    let current = world
        .resource::<Assets<StandardMaterial>>()
        .get(&current_handle)
        .ok_or("Missing UV material asset")?
        .clone();
    let original = if let Some(targets) = world.get_resource::<crate::ProjectionTargets>() {
        targets.document_material(
            entity,
            &current_handle,
            &current,
            world.resource::<Assets<StandardMaterial>>(),
        )?
    } else {
        current
    };
    let projected = world
        .get_resource::<crate::ProjectionTargets>()
        .is_some_and(|t| !t.document_layers(entity).is_empty());
    if projected {
        world
            .resource::<crate::ProjectionTargets>()
            .validate_layer_migration(world, entity)?;
        crate::project_uv::capture_layer_migration(world, entity, &original)?;
    } else {
        crate::project_uv::capture(world, entity, &original)?;
    }
    let texture = world
        .get::<MeshPaintTexture>(entity)
        .ok_or("Missing UV image owner")?;
    let images = world.resource::<Assets<Image>>();
    let r = world.resource::<MeshPaintingResource>();
    if images
        .get(&texture.image_handle)
        .and_then(|i| i.data.as_ref())
        != r.shared_display_bytes.get(&p.mesh_id)
        || texture
            .original_texture
            .as_ref()
            .and_then(|h| images.get(h))
            .and_then(|i| i.data.as_ref())
            != r.shared_original_bytes
                .get(&p.mesh_id)
                .and_then(|v| v.as_ref())
    {
        return Err(
            "UV migration refuses externally changed direct or original image bytes".into(),
        );
    }
    let raw = world
        .resource::<MeshPaintingResource>()
        .get_uv_surface(p.mesh_id)
        .ok_or("Wait for UV setup")?
        .atlas
        .surface()
        .pixels()
        .to_vec();
    let projection = world
        .get_resource::<crate::ProjectionTargets>()
        .map(|t| t.document_layers(entity))
        .unwrap_or_default();
    let mut data = Vec::new();
    for (plane, pixels) in projection {
        if pixels.len() != w as usize * h as usize {
            if pixels.iter().flatten().all(|v| v.to_bits() == 0) {
                continue;
            }
            return Err("Legacy projection dimensions differ; keep the project unchanged and use a matching UV receiver".into());
        }
        data.push(UvLayerDocument {
            meta: UvLayerMeta {
                id: data.len() as u32,
                name: format!("Canvas {plane} snapshot"),
                blend_mode: UvBlendMode::Normal,
                visible: true,
                opacity: 1.,
                locked: false,
                mask_enabled: false,
                paint_target: UvPaintTarget::Color,
            },
            mask: None,
            pixels,
        });
    }
    data.push(UvLayerDocument {
        meta: UvLayerMeta {
            id: data.len() as u32,
            name: "UV Layer 1".into(),
            blend_mode: UvBlendMode::Normal,
            visible: true,
            opacity: 1.,
            locked: false,
            mask_enabled: false,
            paint_target: UvPaintTarget::Color,
        },
        mask: None,
        pixels: raw,
    });
    for layer in &data {
        if layer
            .pixels
            .iter()
            .any(|p| p[3] > 0. && p[..3].iter().any(|v| *v > p[3]))
        {
            return Err("Legacy pixels are not associated RGBA; migration refuses normalization or data loss".into());
        }
    }
    let doc = UvLayersDocument {
        width: w,
        height: h,
        seam_padding: world
            .resource::<MeshPaintingResource>()
            .get_uv_surface(p.mesh_id)
            .unwrap()
            .seam_padding,
        active_layer: data.len() as u32 - 1,
        next_id: data.len() as u32,
        compositor: UV_COMPOSITOR.into(),
        layers: data,
    };
    let layers = UvLayers::restore(doc)?;
    let t = world
        .get::<MeshPaintTexture>(entity)
        .ok_or("Missing UV texture owner")?
        .clone();
    let display = world
        .resource::<Assets<Image>>()
        .get(&t.image_handle)
        .and_then(|i| i.data.clone())
        .ok_or("Missing CPU UV display")?;
    let original_bytes = t
        .original_texture
        .as_ref()
        .and_then(|h| world.resource::<Assets<Image>>().get(h))
        .and_then(|i| i.data.clone());
    {
        let mut r = world.resource_mut::<MeshPaintingResource>();
        if projected {
            r.accept_layer_migration(p.mesh_id);
        }
        r.uv_layers.insert(p.mesh_id, layers);
        r.shared_display_bytes.insert(p.mesh_id, display);
        r.shared_original_bytes.insert(p.mesh_id, original_bytes);
        r.sync_active_layer(p.mesh_id);
    }
    if let Some(mut targets) = world.get_resource_mut::<crate::ProjectionTargets>() {
        targets.detach_shared(entity, (w, h));
    }
    let mut material = world.resource_mut::<Assets<StandardMaterial>>();
    let m = material.get_mut(&current_handle).unwrap();
    m.base_color = t.original_color_source;
    m.base_color_texture = t.original_texture.clone();
    drop(material);
    world.get_mut::<MeshPaintTexture>(entity).unwrap().has_paint = false;
    if let Some(mut p) = world.get_mut::<crate::ProjectionTarget>(entity) {
        p.storage_mode = painting::MeshStorageMode::UvAtlas { resolution: (w, h) };
    }
    if let Some(mut projection) = world.get_resource_mut::<crate::ProjectionMode>() {
        projection.live_projection = false;
    }
    world.resource_mut::<PaintMode>().target_notice=Some("UV layers enabled: exact raw textures retained; display now uses linear Normal compositing. Imported projection snapshots are independent of their source canvases. Apply commits to the selected UV layer.".into());
    Ok(())
}

pub(crate) fn command(world: &mut World, command: &PaintCommand) -> bool {
    let shared_selected = selected(world).is_some_and(|(_, id)| {
        world
            .resource::<MeshPaintingResource>()
            .uv_layers(id)
            .is_some()
    });
    if world
        .get_resource::<crate::projection_painting::LiveUvPreview>()
        .is_some_and(|p| p.active())
        && matches!(
            command,
            PaintCommand::CancelUvProjection | PaintCommand::SetLiveProjection { enabled: false }
        )
    {
        crate::projection_painting::cancel_uv_preview(world);
        return true;
    }
    if shared_selected
        && !matches!(
            command,
            PaintCommand::ProjectToScene | PaintCommand::CancelUvProjection
        )
        && world
            .get_resource::<bevy::ecs::message::Messages<crate::ProjectionEvent>>()
            .is_some_and(|m| {
                m.iter_current_update_messages()
                    .any(|event| matches!(event, crate::ProjectionEvent::ProjectToScene))
            })
    {
        crate::direct_uv_tool::error(
            world,
            "Wait for the pending UV Apply commit before changing source or target state",
        );
        return true;
    }
    let preview_active = world
        .get_resource::<crate::projection_painting::LiveUvPreview>()
        .is_some_and(|p| p.active());
    if matches!(command, PaintCommand::CancelUvProjection) {
        crate::projection_painting::cancel_uv_preview(world);
        return true;
    }
    if shared_selected {
        if let PaintCommand::SetLiveProjection { enabled } = command {
            if *enabled {
                if let Err(message) = crate::projection_painting::begin_uv_preview(world) {
                    crate::direct_uv_tool::error(world, &message);
                }
            } else {
                crate::projection_painting::cancel_uv_preview(world);
            }
            return true;
        }
    }
    if preview_active
        && matches!(
            command,
            PaintCommand::UvLayers { .. } | PaintCommand::SetTarget { .. }
        )
    {
        crate::direct_uv_tool::error(
            world,
            "Apply or cancel the UV preview before changing its layer, receiver, history or painting target",
        );
        return true;
    }
    if preview_active
        && matches!(command, PaintCommand::ProjectToScene)
        && world
            .get_resource::<crate::projection_painting::PendingUvApplies>()
            .is_some_and(|p| p.bytes() > 0)
    {
        crate::direct_uv_tool::error(world, "The UV preview Apply is already pending");
        return true;
    }
    if matches!(command, PaintCommand::ProjectToScene)
        && selected(world).is_some_and(|(_, id)| {
            world
                .resource::<MeshPaintingResource>()
                .uv_layers(id)
                .is_some()
        })
        && (crate::brush_presets::active(world)
            || world
                .get_resource::<crate::FrontendScenePointerInput>()
                .is_some_and(|s| s.has_scene_press()))
    {
        crate::direct_uv_tool::error(
            world,
            "Finish or cancel the source or UV stroke before Apply",
        );
        return true;
    }
    if shared_selected && matches!(command, PaintCommand::ProjectToScene) {
        if let Err(message) = crate::projection_painting::admit_uv_apply(world) {
            crate::direct_uv_tool::error(world, &message);
            return true;
        }
    }
    let PaintCommand::UvLayers { command } = command else {
        return false;
    };
    let locked = crate::brush_presets::active(world)
        || world
            .get_resource::<crate::FrontendScenePointerInput>()
            .is_some_and(|s| s.has_scene_press())
        || world
            .get_resource::<bevy::ecs::message::Messages<crate::ProjectionEvent>>()
            .is_some_and(|m| !m.is_empty());
    let result = (|| -> Result<(), String> {
        if locked {
            return Err(
                "Finish or cancel painting and pending projection before editing UV layers".into(),
            );
        }
        crate::mesh_painting_system::sync_mesh_paint_owners(world);
        if let Command::SelectReceiver { mesh_id } = command {
            let matches: Vec<_> = world
                .query::<(Entity, &PaintableMesh)>()
                .iter(world)
                .filter(|(_, p)| p.mesh_id == *mesh_id)
                .map(|(e, _)| e)
                .collect();
            if matches.len() != 1 {
                return Err("Unknown or ambiguous UV receiver".into());
            }
            world.resource_mut::<PaintMode>().direct_target = Some(matches[0]);
            return Ok(());
        }
        let (e, id) = selected(world).ok_or("Select the UV receiver first")?;
        if world
            .resource::<MeshPaintingResource>()
            .history_conflicted(id)
            && !matches!(command, Command::Enable)
        {
            return Err("UV owner is conflicted; reopen the owned project".into());
        }
        if matches!(command, Command::Enable) {
            return enable(world, e);
        }
        if matches!(command, Command::Undo | Command::Redo) {
            let success = if matches!(command, Command::Undo) {
                crate::undo_mesh_paint(world, e)
            } else {
                crate::redo_mesh_paint(world, e)
            };
            return if success {
                Ok(())
            } else {
                Err("UV layer history is unavailable or conflicted".into())
            };
        }
        let op = match command {
            Command::AddMask { layer_id } => UvLayerOp::AddMask(*layer_id),
            Command::RemoveMask { layer_id } => UvLayerOp::RemoveMask(*layer_id),
            Command::MaskEnabled { layer_id, enabled } => {
                UvLayerOp::MaskEnabled(*layer_id, *enabled)
            }
            Command::PaintTarget { layer_id, target } => UvLayerOp::PaintTarget(
                *layer_id,
                match target {
                    WireTarget::Color => UvPaintTarget::Color,
                    WireTarget::Mask => UvPaintTarget::Mask,
                },
            ),
            Command::Create { name } => UvLayerOp::Create(name.clone()),
            Command::Duplicate { layer_id } => UvLayerOp::Duplicate(*layer_id),
            Command::Delete { layer_id } => UvLayerOp::Delete(*layer_id),
            Command::Select { layer_id } => UvLayerOp::Select(*layer_id),
            Command::Rename { layer_id, name } => UvLayerOp::Rename(*layer_id, name.clone()),
            Command::Reorder {
                layer_id,
                new_index,
            } => UvLayerOp::Reorder(*layer_id, *new_index),
            Command::Visible { layer_id, visible } => UvLayerOp::Visible(*layer_id, *visible),
            Command::Opacity { layer_id, opacity } => UvLayerOp::Opacity(*layer_id, *opacity),
            Command::Lock { layer_id, locked } => UvLayerOp::Lock(*layer_id, *locked),
            Command::BlendMode { layer_id, mode } => UvLayerOp::BlendMode(
                *layer_id,
                match mode {
                    WireBlend::Normal => UvBlendMode::Normal,
                    WireBlend::Multiply => UvBlendMode::Multiply,
                    WireBlend::Screen => UvBlendMode::Screen,
                    WireBlend::Overlay => UvBlendMode::Overlay,
                },
            ),
            _ => unreachable!(),
        };
        world
            .resource_mut::<MeshPaintingResource>()
            .edit_uv_layer(id, op)?;
        Ok(())
    })();
    if let Err(message) = result {
        crate::direct_uv_tool::error(world, message);
    }
    true
}

pub(crate) fn install(
    world: &mut World,
    entity: Entity,
    id: u32,
    document: UvLayersDocument,
    material: Handle<StandardMaterial>,
) {
    let active = document
        .layers
        .iter()
        .find(|l| l.meta.id == document.active_layer)
        .unwrap()
        .working_pixels();
    let original = world
        .resource::<Assets<StandardMaterial>>()
        .get(&material)
        .unwrap()
        .clone();
    let c = original.base_color.to_linear();
    let color = [c.red, c.green, c.blue, c.alpha];
    let original_bytes = original
        .base_color_texture
        .as_ref()
        .and_then(|h| world.resource::<Assets<Image>>().get(h))
        .and_then(|i| i.data.clone());
    let display = composite_display(&document, color, original_bytes.as_deref());
    let bound = document.composite().iter().any(|p| p[3] > 0.);
    crate::project_uv::install(
        world,
        entity,
        id,
        crate::project_uv::DirectUvDocument {
            width: document.width,
            height: document.height,
            seam_padding: document.seam_padding,
            pixels: active,
            display_bound: false,
            original_linear_color: color,
        },
        material.clone(),
    );
    let image = world
        .get::<MeshPaintTexture>(entity)
        .unwrap()
        .image_handle
        .clone();
    world
        .resource_mut::<Assets<Image>>()
        .get_mut(&image)
        .unwrap()
        .data = Some(display.clone());
    if bound {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let m = materials.get_mut(&material).unwrap();
        m.base_color = Color::WHITE;
        m.base_color_texture = Some(image);
    }
    world.get_mut::<MeshPaintTexture>(entity).unwrap().has_paint = bound;
    let mut r = world.resource_mut::<MeshPaintingResource>();
    r.uv_layers.insert(id, UvLayers::restore(document).unwrap());
    r.shared_display_bytes.insert(id, display);
    r.shared_original_bytes.insert(id, original_bytes);
}
