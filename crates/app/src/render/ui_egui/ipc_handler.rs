use bevy::prelude::*;
use pentimento_egui_ui::apply_bevy_message;
use pentimento_scene::{DepthViewSettings, OutboundUiMessages};

use super::resources::EguiFrontendState;

pub fn sync_bevy_messages(
    mut outbound: ResMut<OutboundUiMessages>,
    mut frontend: ResMut<EguiFrontendState>,
    depth: Option<Res<DepthViewSettings>>,
) {
    for message in outbound.drain() {
        apply_bevy_message(&mut frontend.snapshot, message);
    }
    if let Some(depth) = depth {
        frontend.snapshot.depth_view_enabled = depth.enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pentimento_egui_ui::{egui, show_root_ui};
    use pentimento_ipc::UiToBevy;

    struct ToolbarHarness {
        app: App,
        context: egui::Context,
        time: f64,
    }

    impl ToolbarHarness {
        fn new() -> Self {
            let mut app = App::new();
            app.init_resource::<OutboundUiMessages>()
                .init_resource::<EguiFrontendState>()
                .init_resource::<DepthViewSettings>()
                .add_systems(Update, sync_bevy_messages);
            let context = egui::Context::default();
            context.all_styles_mut(|style| style.animation_time = 0.0);
            Self {
                app,
                context,
                time: 0.0,
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> (egui::FullOutput, Vec<UiToBevy>) {
            self.app.update();
            self.time += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 1800.0),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let mut commands = Vec::new();
            let frontend = &mut *self.app.world_mut().resource_mut::<EguiFrontendState>();
            let mut output = self.context.run_ui(input, |ui| {
                commands = show_root_ui(ui.ctx(), &mut frontend.snapshot, &mut frontend.runtime);
            });
            output.textures_delta.clear();
            (output, commands)
        }

        fn click(&mut self, label: &str) -> Vec<UiToBevy> {
            self.frame(vec![]);
            let (output, _) = self.frame(vec![]);
            fn locate(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    egui::epaint::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| locate(shape, label))
                    }
                    _ => None,
                }
            }
            let position = output
                .shapes
                .iter()
                .find_map(|shape| locate(&shape.shape, label))
                .unwrap_or_else(|| panic!("Missing toolbar widget: {label}"));
            self.frame(vec![egui::Event::PointerMoved(position)]);
            self.frame(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }]);
            self.frame(vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }])
            .1
        }
    }

    #[test]
    fn depth_toolbar_roundtrips_authoritative_scene_state() {
        let mut toolbar = ToolbarHarness::new();
        for (label, enabled) in [("Depth View: Off", true), ("Depth View: On", false)] {
            let commands: Vec<_> = toolbar
                .click(label)
                .into_iter()
                .filter(|command| matches!(command, UiToBevy::SetDepthView { .. }))
                .collect();
            assert_eq!(commands.len(), 1);
            assert!(
                matches!(commands[0], UiToBevy::SetDepthView { enabled: actual } if actual == enabled)
            );
            crate::render::ui_commands::dispatch_ui_commands(toolbar.app.world_mut(), commands);
            toolbar.app.update();
            assert_eq!(
                toolbar.app.world().resource::<DepthViewSettings>().enabled,
                enabled
            );
            assert_eq!(
                toolbar
                    .app
                    .world()
                    .resource::<EguiFrontendState>()
                    .snapshot
                    .depth_view_enabled,
                enabled
            );
        }
        // A change from another controller also reaches the real toolbar.
        toolbar
            .app
            .world_mut()
            .resource_mut::<DepthViewSettings>()
            .enabled = true;
        let commands = toolbar.click("Depth View: On");
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, UiToBevy::SetDepthView { enabled: false }))
        );
    }
}
