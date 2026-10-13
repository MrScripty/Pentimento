//! Optional CPU evidence renderer for the *actual* egui output, including its
//! font atlas, triangle meshes and clip rectangles. This is not a Bevy/GPU or
//! stylus qualification. PPM avoids adding a production/development dependency.
use crate::egui;
use std::{collections::HashMap, io::Write, path::Path};

#[derive(Default)]
pub(crate) struct Textures(HashMap<egui::TextureId, egui::ColorImage>);
impl Textures {
    pub(crate) fn apply(&mut self, delta: &mut egui::TexturesDelta) {
        for (id, updates) in delta.set.drain() {
            for update in updates {
                let egui::ImageData::Color(image) = update.image;
                if let Some([x, y]) = update.pos {
                    let target = self
                        .0
                        .get_mut(&id)
                        .expect("partial update needs a full atlas");
                    for row in 0..image.height() {
                        let begin = (y + row) * target.width() + x;
                        target.pixels[begin..begin + image.width()].copy_from_slice(
                            &image.pixels[row * image.width()..(row + 1) * image.width()],
                        );
                    }
                } else {
                    self.0.insert(id, (*image).clone());
                }
            }
        }
        for id in delta.free.drain() {
            self.0.remove(&id);
        }
    }

    pub(crate) fn write_frame(
        &self,
        path: &Path,
        ctx: &egui::Context,
        output: egui::FullOutput,
        size: egui::Vec2,
    ) {
        let width = size.x as usize;
        let height = size.y as usize;
        let mut pixels = vec![[22.0, 24.0, 28.0]; width * height];
        for clipped in ctx.tessellate(output.shapes, output.pixels_per_point) {
            let egui::epaint::Primitive::Mesh(mesh) = clipped.primitive else {
                panic!("unsupported paint callback in CPU capture");
            };
            let texture = self.0.get(&mesh.texture_id).expect("actual egui texture");
            let clip = clipped
                .clip_rect
                .intersect(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
            for triangle in mesh.indices.chunks_exact(3) {
                let mut v = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                let edge = |a: egui::Pos2, b: egui::Pos2, p: egui::Pos2| {
                    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
                };
                if edge(v[0].pos, v[1].pos, v[2].pos) < 0.0 {
                    v.swap(1, 2);
                }
                let area = edge(v[0].pos, v[1].pos, v[2].pos);
                if area.abs() < 0.0001 {
                    continue;
                }
                let bounds =
                    egui::Rect::from_points(&[v[0].pos, v[1].pos, v[2].pos]).intersect(clip);
                for y in bounds.min.y.floor().max(0.0) as usize
                    ..(bounds.max.y.ceil() as usize).min(height)
                {
                    for x in bounds.min.x.floor().max(0.0) as usize
                        ..(bounds.max.x.ceil() as usize).min(width)
                    {
                        let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                        if !clip.contains(p) {
                            continue;
                        }
                        let w = [
                            edge(v[1].pos, v[2].pos, p) / area,
                            edge(v[2].pos, v[0].pos, p) / area,
                            edge(v[0].pos, v[1].pos, p) / area,
                        ];
                        // Include shared edges once (top-left fill convention).
                        let top_left =
                            |a: egui::Pos2, b: egui::Pos2| b.y < a.y || (b.y == a.y && b.x > a.x);
                        if (0..3).any(|i| {
                            w[i] < 0.0
                                || (w[i] == 0.0
                                    && !top_left(v[(i + 1) % 3].pos, v[(i + 2) % 3].pos))
                        }) {
                            continue;
                        }
                        let u = (0..3).map(|i| v[i].uv.x * w[i]).sum::<f32>();
                        let t = (0..3).map(|i| v[i].uv.y * w[i]).sum::<f32>();
                        // Bilinear font-atlas sampling, as used by egui's renderers.
                        let sx = (u * texture.width() as f32 - 0.5)
                            .clamp(0.0, (texture.width() - 1) as f32);
                        let sy = (t * texture.height() as f32 - 0.5)
                            .clamp(0.0, (texture.height() - 1) as f32);
                        let (ix, iy) = (sx.floor() as usize, sy.floor() as usize);
                        let (fx, fy) = (sx.fract(), sy.fract());
                        let samples = [
                            (ix, iy, (1.0 - fx) * (1.0 - fy)),
                            ((ix + 1).min(texture.width() - 1), iy, fx * (1.0 - fy)),
                            (ix, (iy + 1).min(texture.height() - 1), (1.0 - fx) * fy),
                            (
                                (ix + 1).min(texture.width() - 1),
                                (iy + 1).min(texture.height() - 1),
                                fx * fy,
                            ),
                        ];
                        let rgba: [f32; 4] = std::array::from_fn(|channel| {
                            let tint = (0..3)
                                .map(|i| v[i].color.to_array()[channel] as f32 * w[i])
                                .sum::<f32>()
                                / 255.0;
                            tint * samples
                                .iter()
                                .map(|(tx, ty, weight)| {
                                    texture.pixels[ty * texture.width() + tx].to_array()[channel]
                                        as f32
                                        * weight
                                })
                                .sum::<f32>()
                        });
                        let pixel = &mut pixels[y * width + x];
                        for channel in 0..3 {
                            pixel[channel] =
                                rgba[channel] + pixel[channel] * (1.0 - rgba[3] / 255.0);
                        }
                    }
                }
            }
        }
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        writeln!(file, "P6\n{width} {height}\n255").unwrap();
        for pixel in pixels {
            file.write_all(&pixel.map(|c| c.clamp(0.0, 255.0).round() as u8))
                .unwrap();
        }
    }
}
