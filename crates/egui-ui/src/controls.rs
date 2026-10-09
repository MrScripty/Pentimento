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
