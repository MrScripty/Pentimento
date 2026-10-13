//! Compare exact regional UV presentation against a full composite followed by
//! cropping. `cargo run -p painting --release --example uv_region_profile`.
use painting::uv_layers::*;
use std::{hint::black_box, time::Instant};

fn main() {
    let size = 1024;
    let count = size * size;
    let document = UvLayersDocument {
        width: size as u32,
        height: size as u32,
        seam_padding: 2,
        active_layer: 3,
        next_id: 4,
        compositor: UV_MASK_COMPOSITOR.into(),
        layers: (0..4)
            .map(|id| UvLayerDocument {
                meta: UvLayerMeta {
                    id,
                    name: format!("Layer {id}"),
                    visible: true,
                    opacity: 0.7,
                    locked: false,
                    blend_mode: UvBlendMode::Normal,
                    mask_enabled: false,
                    paint_target: UvPaintTarget::Color,
                },
                pixels: (0..count)
                    .map(|i| {
                        let a = (i % 31) as f32 / 30.;
                        [a * 0.2, a * 0.4, a * 0.6, a]
                    })
                    .collect(),
                mask: None,
            })
            .collect(),
    };
    document.validate().unwrap();
    println!("workload,operations,median_ms,p95_ms");
    let expected: Vec<_> = document
        .composite()
        .chunks_exact(size)
        .skip(384)
        .take(128)
        .flat_map(|row| row[256..384].iter().copied())
        .collect();
    for regional in [false, true] {
        let mut times = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            for _ in 0..8 {
                let result = if regional {
                    document.composite_active_region(None, 256, 384, 128, 128)
                } else {
                    document
                        .composite()
                        .chunks_exact(size)
                        .skip(384)
                        .take(128)
                        .flat_map(|row| row[256..384].iter().copied())
                        .collect()
                };
                black_box(&result);
            }
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{},8,{:.3},{:.3}",
            if regional {
                "region_128_square"
            } else {
                "full_atlas_then_crop"
            },
            times[3],
            times[6]
        );
    }
    assert!(same_uv_pixels(
        &expected,
        &document.composite_active_region(None, 256, 384, 128, 128)
    ));
    // Material binding is exact, but its worst case still scans empty layers.
    let mut empty = document;
    for layer in &mut empty.layers {
        layer.pixels.fill([0.; 4]);
    }
    for sparse in [false, true] {
        if sparse {
            empty.layers[3].pixels[count - 1] = [0.2, 0.3, 0.4, 1.];
        }
        let start = Instant::now();
        for _ in 0..16 {
            assert_eq!(black_box(empty.has_visible_paint(None)), sparse);
        }
        println!(
            "binding_{}_16_checks,16,{:.3},n/a",
            if sparse { "last_pixel" } else { "empty" },
            start.elapsed().as_secs_f64() * 1000.
        );
    }
}
