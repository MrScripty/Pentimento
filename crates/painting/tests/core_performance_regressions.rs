//! Exact output/history guards for incremental brush presentation.
use painting::uv_layers::*;
use painting::{PaintingPipeline, TileCoord};

#[test]
fn idle_canvas_has_no_composite_or_upload_work() {
    let mut pipeline = PaintingPipeline::new(1024, 1024);
    pipeline.clear([0.2, 0.3, 0.4, 1.]);
    assert_eq!(pipeline.take_dirty_tiles().len(), 64);
    let pixels = pipeline.surface_as_bytes().to_vec();
    for _ in 0..8 {
        assert!(!pipeline.has_dirty_tiles());
        assert!(pipeline.take_dirty_tiles().is_empty());
        assert_eq!(pipeline.surface_as_bytes(), pixels);
    }
}

#[test]
fn small_canvas_stroke_uploads_one_tile_and_history_remains_exact() {
    let mut pipeline = PaintingPipeline::new(300, 270);
    pipeline.clear([0.2, 0.3, 0.4, 1.]);
    pipeline.layers.add_layer("Ink".into());
    pipeline.take_dirty_tiles();
    let before = pipeline.surface_as_bytes().to_vec();
    pipeline.begin_stroke(7, 1, 0);
    pipeline.stroke_to(40., 40., 0.2);
    pipeline.end_stroke();
    assert_eq!(pipeline.take_dirty_tiles(), [TileCoord { x: 0, y: 0 }]);
    let after = pipeline.surface_as_bytes().to_vec();
    assert_ne!(after, before);
    assert_eq!(
        pipeline.layers.document_composite_pixels(),
        bytemuck::cast_slice::<u8, [f32; 4]>(&after)
    );
    pipeline.begin_stroke(7, 2, 0);
    pipeline.stroke_to(285., 258., 0.5);
    pipeline.take_dirty_tiles();
    pipeline.cancel_stroke();
    pipeline.take_dirty_tiles();
    assert_eq!(pipeline.surface_as_bytes(), after);
    assert!(pipeline.undo());
    assert_eq!(pipeline.take_dirty_tiles(), [TileCoord { x: 0, y: 0 }]);
    assert_eq!(pipeline.surface_as_bytes(), before);
    assert!(pipeline.redo());
    pipeline.take_dirty_tiles();
    assert_eq!(pipeline.surface_as_bytes(), after);
}

#[test]
fn metadata_and_public_pixel_edits_invalidate_full_canvas() {
    let mut pipeline = PaintingPipeline::new(300, 270);
    pipeline.clear([0.2, 0.3, 0.4, 1.]);
    let top = pipeline.layers.add_layer("Top".into());
    pipeline.clear([0.6, 0.2, 0.1, 0.8]);
    pipeline.take_dirty_tiles();
    pipeline.layers.set_opacity(top, 0.35);
    assert!(pipeline.has_dirty_tiles());
    assert_eq!(pipeline.take_dirty_tiles().len(), 9);
    assert_eq!(
        bytemuck::cast_slice::<u8, [f32; 4]>(pipeline.surface_as_bytes()),
        pipeline.layers.document_composite_pixels()
    );
    pipeline
        .layers
        .layer_mut(top)
        .unwrap()
        .surface
        .surface_mut()
        .set_pixel(299, 269, [0.4, 0.2, 0.7, 1.]);
    assert_eq!(pipeline.take_dirty_tiles().len(), 9);
    assert_eq!(
        bytemuck::cast_slice::<u8, [f32; 4]>(pipeline.surface_as_bytes()),
        pipeline.layers.document_composite_pixels()
    );
    pipeline.layers.set_visibility(top, false);
    assert_eq!(pipeline.take_dirty_tiles().len(), 9);
    assert_eq!(
        bytemuck::cast_slice::<u8, [f32; 4]>(pipeline.surface_as_bytes()),
        pipeline.layers.document_composite_pixels()
    );
    pipeline.layers.set_visibility(top, true);
    assert_eq!(pipeline.take_dirty_tiles().len(), 9);
    pipeline.clear([0.; 4]);
    assert_eq!(pipeline.take_dirty_tiles().len(), 9);
    assert_eq!(
        bytemuck::cast_slice::<u8, [f32; 4]>(pipeline.surface_as_bytes()),
        pipeline.layers.document_composite_pixels()
    );
    pipeline.layers.reorder(top, 0);
    assert_eq!(pipeline.take_dirty_tiles().len(), 9);
    pipeline.layers.remove_layer(top);
    assert_eq!(pipeline.take_dirty_tiles().len(), 9);
    assert!(!pipeline.has_dirty_tiles());
}

fn document(mode: UvBlendMode, target: UvPaintTarget) -> UvLayersDocument {
    let count = 17 * 13;
    UvLayersDocument {
        width: 17,
        height: 13,
        seam_padding: 2,
        active_layer: 2,
        next_id: 4,
        compositor: UV_MASK_COMPOSITOR.into(),
        layers: (0..4)
            .map(|id| UvLayerDocument {
                meta: UvLayerMeta {
                    id,
                    name: format!("Layer {id}"),
                    visible: id != 3,
                    opacity: 0.73,
                    locked: false,
                    blend_mode: mode,
                    mask_enabled: id != 0,
                    paint_target: if id == 2 {
                        target
                    } else {
                        UvPaintTarget::Color
                    },
                },
                pixels: (0..count)
                    .map(|i| {
                        let a = ((i + id as usize) % 11) as f32 / 10.;
                        if a == 0. {
                            [0.7, -0., 0.2, 0.]
                        } else {
                            [a * 0.8, a * 0.3, a * 0.1, a]
                        }
                    })
                    .collect(),
                mask: Some(
                    (0..count)
                        .map(|i| ((i + id as usize) % 7) as f32 / 6.)
                        .collect(),
                ),
            })
            .collect(),
    }
}

// Deliberately retain the pre-optimization scalar algorithm as an independent
// oracle. A regional result must not merely agree with another sliced helper.
fn reference(document: &UvLayersDocument, active: Option<&[[f32; 4]]>) -> Vec<[f32; 4]> {
    let mut output = vec![[0.; 4]; (document.width * document.height) as usize];
    for layer in &document.layers {
        if !layer.meta.visible || layer.meta.opacity == 0. {
            continue;
        }
        let pixels = if layer.meta.id == document.active_layer
            && layer.meta.paint_target == UvPaintTarget::Color
        {
            active.unwrap_or(&layer.pixels)
        } else {
            &layer.pixels
        };
        for (i, (dst, src)) in output.iter_mut().zip(pixels).enumerate() {
            let coverage = if layer.meta.mask_enabled {
                if layer.meta.id == document.active_layer
                    && layer.meta.paint_target == UvPaintTarget::Mask
                {
                    active.map_or_else(
                        || layer.mask.as_ref().unwrap()[i],
                        |p| uv_mask_coverage(p[i]),
                    )
                } else {
                    layer.mask.as_ref().unwrap()[i]
                }
            } else {
                1.
            };
            let src = if layer.meta.mask_enabled {
                src.map(|v| v * coverage)
            } else {
                *src
            };
            *dst = uv_blend_over(
                src.map(|v| v * layer.meta.opacity),
                *dst,
                layer.meta.blend_mode,
            );
        }
    }
    output
}

#[test]
fn regional_uv_composite_matches_original_bits_for_every_blend_and_target() {
    for mode in [
        UvBlendMode::Normal,
        UvBlendMode::Multiply,
        UvBlendMode::Screen,
        UvBlendMode::Overlay,
    ] {
        for target in [UvPaintTarget::Color, UvPaintTarget::Mask] {
            let document = document(mode, target);
            document.validate().unwrap();
            let active: Vec<_> = (0..17 * 13)
                .map(|i| {
                    let a = (i % 5) as f32 / 4.;
                    [0.3 * a, 0.2 * a, 0.1 * a, a]
                })
                .collect();
            for active in [None, Some(active.as_slice())] {
                let expected = reference(&document, active);
                assert!(same_uv_pixels(
                    &document.composite_active(active),
                    &expected
                ));
                assert_eq!(
                    document.has_visible_paint(active),
                    expected.iter().any(|p| p[3] > 0.)
                );
                for (x, y, w, h) in [
                    (0, 0, 17, 13),
                    (3, 2, 6, 7),
                    (16, 12, 20, 20),
                    (8, 9, u32::MAX, u32::MAX),
                    (17, 0, 1, 1),
                    (0, 13, 1, 1),
                    (0, 0, 0, 7),
                ] {
                    let actual = document.composite_active_region(active, x, y, w, h);
                    let mut region = Vec::new();
                    for row in y..y.saturating_add(h).min(13) {
                        for col in x..x.saturating_add(w).min(17) {
                            region.push(expected[(row * 17 + col) as usize]);
                        }
                    }
                    assert!(
                        same_uv_pixels(&actual, &region),
                        "{mode:?} {target:?} rectangle {x},{y},{w},{h}"
                    );
                }
            }
        }
    }
}

#[test]
fn visible_paint_follows_last_pixel_erase_hidden_layers_and_mask_override() {
    let mut document = document(UvBlendMode::Overlay, UvPaintTarget::Mask);
    for layer in &mut document.layers {
        layer.pixels.fill([0.7, 0.2, 0.1, 0.]);
    }
    let check = |d: &UvLayersDocument, active| {
        assert_eq!(
            d.has_visible_paint(active),
            reference(d, active).iter().any(|p| p[3] > 0.)
        )
    };
    check(&document, None);
    assert!(!document.has_visible_paint(None));
    document.layers[2].pixels[220] = [0.5, 0.2, 0.1, 1.];
    document.layers[2].mask.as_mut().unwrap().fill(0.);
    check(&document, None);
    assert!(!document.has_visible_paint(None));
    let mut active = vec![[0., 0., 0., 1.]; 221];
    active[220] = [0., 0., 0., 0.]; // Erasing an active mask reveals white.
    check(&document, Some(&active));
    assert!(document.has_visible_paint(Some(&active)));
    document.layers[2].meta.visible = false;
    check(&document, Some(&active));
    assert!(!document.has_visible_paint(Some(&active)));
    document.layers[2].meta.visible = true;
    document.layers[2].meta.opacity = 0.;
    check(&document, Some(&active));
    assert!(!document.has_visible_paint(Some(&active)));
}
