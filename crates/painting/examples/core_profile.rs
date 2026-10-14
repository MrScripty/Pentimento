//! Deterministic CPU-only workloads; run with `cargo run -p painting --release
//! --example core_profile`. Timings exclude fixture setup and output checksums.
use painting::uv_layers::{
    UV_MASK_COMPOSITOR, UvBlendMode, UvLayerDocument, UvLayerMeta, UvLayersDocument, UvPaintTarget,
};
use painting::{BlendMode, TiledSurface};
use std::{hint::black_box, time::Instant};

fn report(name: &str, mut values: Vec<f64>, operations: usize, checksum: u64) {
    values.sort_by(f64::total_cmp);
    println!(
        "{name},{operations},{:.3},{:.3},{checksum:016x}",
        values[values.len() / 2],
        values[(values.len() * 95 / 100).min(values.len() - 1)]
    );
}
fn checksum(pixels: &[[f32; 4]]) -> u64 {
    pixels.iter().flatten().fold(0xcbf29ce484222325, |h, v| {
        (h ^ u64::from(v.to_bits())).wrapping_mul(0x100000001b3)
    })
}
fn main() {
    let samples = std::env::args()
        .nth(1)
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(7)
        .max(1);
    println!("workload,operations,median_ms,p95_ms,checksum");
    for shared in [false, true] {
        let mut times = Vec::new();
        let mut hash = 0;
        for _ in 0..samples {
            let mut surface = TiledSurface::with_default_tile_size(1024, 1024);
            surface.surface_mut().clear([0.15, 0.2, 0.1, 0.5]);
            let start = Instant::now();
            for i in 0..128 {
                let x = 200. + (i % 16) as f32 * 39.;
                let y = 200. + (i / 16) as f32 * 79.;
                if shared {
                    black_box(surface.apply_dab_ellipse_uv(
                        x,
                        y,
                        64.,
                        [0.3, 0.5, 0.7, 0.8],
                        0.45,
                        0.6,
                        BlendMode::Normal,
                        0.3,
                        0.65,
                    ));
                } else {
                    black_box(surface.apply_dab(
                        x,
                        y,
                        64.,
                        [0.3, 0.5, 0.7, 0.8],
                        0.45,
                        0.6,
                        BlendMode::Normal,
                    ));
                }
            }
            times.push(start.elapsed().as_secs_f64() * 1000.);
            hash = checksum(surface.surface().pixels());
        }
        report(
            if shared {
                "uv_ellipse_128_dabs"
            } else {
                "canvas_circle_128_dabs"
            },
            times,
            128,
            hash,
        );
    }
    for idle in [true, false] {
        let mut times = Vec::new();
        let mut uploaded = 0;
        for _ in 0..samples {
            let mut pipeline = painting::PaintingPipeline::new(1024, 1024);
            pipeline.clear([0.2, 0.3, 0.4, 1.]);
            pipeline.take_dirty_tiles();
            pipeline.begin_stroke(0, 1, 0);
            let start = Instant::now();
            for i in 0..32 {
                if !idle {
                    pipeline.stroke_to(40. + i as f32, 40., 0.2);
                }
                uploaded += black_box(pipeline.take_dirty_tiles()).len();
            }
            times.push(start.elapsed().as_secs_f64() * 1000.);
            pipeline.cancel_stroke();
        }
        report(
            if idle {
                "canvas_idle_32_frames"
            } else {
                "canvas_small_stroke_32_frames"
            },
            times,
            32,
            uploaded as u64,
        );
    }
    for masked in [false, true] {
        let count = 512 * 512;
        let document = UvLayersDocument {
            width: 512,
            height: 512,
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
                        blend_mode: if masked && id == 2 {
                            UvBlendMode::Overlay
                        } else {
                            UvBlendMode::Normal
                        },
                        mask_enabled: masked,
                        paint_target: UvPaintTarget::Color,
                    },
                    pixels: (0..count)
                        .map(|i| {
                            let a = (i % 31) as f32 / 30.;
                            [a * 0.2, a * 0.4, a * 0.6, a]
                        })
                        .collect(),
                    mask: masked.then(|| (0..count).map(|i| (i % 17) as f32 / 16.).collect()),
                })
                .collect(),
        };
        document.validate().unwrap();
        let mut times = Vec::new();
        let mut output = Vec::new();
        for _ in 0..samples {
            let start = Instant::now();
            for _ in 0..16 {
                output = black_box(document.composite());
            }
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
        report(
            if masked {
                "uv_masked_4_layers_16_composites"
            } else {
                "uv_normal_4_layers_16_composites"
            },
            times,
            16,
            checksum(&output),
        );
    }
}
