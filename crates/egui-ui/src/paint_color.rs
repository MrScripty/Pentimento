use crate::{controls::paint, egui, state::EguiUiRuntime};
use pentimento_ipc::{PaintBrushSettings, PaintCommand, UiToBevy};

fn hex(color: [f32; 3]) -> String {
    let [r, g, b] = color.map(egui::ecolor::gamma_u8_from_linear_f32);
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn parse_hex(text: &str) -> Option<[f32; 4]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut color = [1.0; 4];
    for (channel, offset) in [0, 2, 4].into_iter().enumerate() {
        color[channel] = egui::ecolor::linear_f32_from_gamma_u8(
            u8::from_str_radix(&digits[offset..offset + 2], 16).ok()?,
        );
    }
    Some(color)
}

pub(crate) fn edit(
    ui: &mut egui::Ui,
    settings: &PaintBrushSettings,
    runtime: &mut EguiUiRuntime,
    enabled: bool,
    commands: &mut Vec<UiToBevy>,
) {
    let accepted = [settings.color[0], settings.color[1], settings.color[2]];
    let draft = runtime
        .paint_hex_color
        .get_or_insert_with(|| (accepted, hex(accepted)));
    if draft.0 != accepted || !enabled {
        *draft = (accepted, hex(accepted));
    }
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("Color");
            let mut rgb = accepted;
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                paint(
                    commands,
                    PaintCommand::SetBrushColor {
                        color: [rgb[0], rgb[1], rgb[2], 1.0],
                    },
                );
                draft.1 = hex(accepted);
            }
            ui.label("Hex color");
            let response = ui.add(
                egui::TextEdit::singleline(&mut draft.1)
                    .id_salt("paint_hex_color")
                    .char_limit(7)
                    .desired_width(76.0),
            );
            if response.lost_focus() {
                // Focusing an untouched field must not quantize a precise linear color.
                if !draft.1.eq_ignore_ascii_case(&hex(accepted)) {
                    if let Some(color) = parse_hex(&draft.1) {
                        paint(commands, PaintCommand::SetBrushColor { color });
                    }
                }
                // Restore the acknowledged color until the native owner accepts the edit.
                draft.1 = hex(accepted);
            }
        });
        if parse_hex(&draft.1).is_none() {
            ui.colored_label(egui::Color32::LIGHT_RED, "Enter a color as #RRGGBB.");
        }
    });
}

pub(crate) fn preview(ui: &mut egui::Ui, settings: &PaintBrushSettings) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
        let color = egui::Color32::from(egui::Rgba::from_rgba_unmultiplied(
            settings.color[0],
            settings.color[1],
            settings.color[2],
            settings.opacity,
        ));
        let mut mesh = egui::epaint::Mesh::default();
        mesh.colored_vertex(rect.center(), color);
        let radius = rect.width() / 2.0;
        for (r, color) in [
            (radius * 0.9 * settings.hardness, color),
            (radius, egui::Color32::TRANSPARENT),
        ] {
            for index in 0..32 {
                let angle = std::f32::consts::TAU * index as f32 / 32.0;
                mesh.colored_vertex(
                    rect.center() + egui::vec2(angle.cos(), angle.sin()) * r,
                    color,
                );
            }
        }
        for index in 0..32 {
            let next = (index + 1) % 32;
            mesh.add_triangle(0, 1 + index, 1 + next);
            mesh.add_triangle(1 + index, 33 + index, 33 + next);
            mesh.add_triangle(1 + index, 33 + next, 1 + next);
        }
        ui.painter().add(egui::epaint::Shape::mesh(mesh));
        ui.vertical(|ui| {
            ui.label("Round tip preview");
            ui.small("Hardness controls edge falloff. Opacity controls coverage.");
        });
    });
}
