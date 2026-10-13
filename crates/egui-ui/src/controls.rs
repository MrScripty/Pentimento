use crate::egui;
use pentimento_ipc::{PaintCommand, SculptCommand, UiToBevy, UvLayerCommand};

pub(crate) fn paint(commands: &mut Vec<UiToBevy>, command: PaintCommand) {
    commands.push(UiToBevy::PaintCommand(command));
}
pub(crate) fn sculpt(commands: &mut Vec<UiToBevy>, command: SculptCommand) {
    commands.push(UiToBevy::SculptCommand(command));
}
pub(crate) fn uv(commands: &mut Vec<UiToBevy>, command: UvLayerCommand) {
    paint(commands, PaintCommand::UvLayers { command });
}
pub(crate) fn button(ui: &mut egui::Ui, enabled: bool, label: &str) -> bool {
    ui.add_enabled(enabled, egui::Button::new(label)).clicked()
}
pub(crate) fn slider(
    ui: &mut egui::Ui,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
) -> Option<f32> {
    let mut edited = value;
    ui.add(egui::Slider::new(&mut edited, range).text(label))
        .changed()
        .then_some(edited)
}

/// A bounded, labelled numeric control. The group wraps as one unit in a toolbar.
pub(crate) fn toolbar_value(
    ui: &mut egui::Ui,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
) -> Option<f32> {
    let mut edited = value;
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::DragValue::new(&mut edited)
                    .range(range)
                    .speed(0.1)
                    .max_decimals(2),
            )
            .changed()
            .then_some(edited)
        })
        .inner
    })
    .inner
}

pub(crate) fn tile(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    ui.add_sized(
        [94.0, 30.0],
        egui::Button::selectable(selected, label).truncate(),
    )
    .on_hover_text(label)
}
