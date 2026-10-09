use crate::{
    controls::{button, sculpt, slider},
    egui, presets,
    state::{EguiUiRuntime, EguiUiSnapshot},
};
use pentimento_ipc::{SculptCommand, SculptFalloff, SculptTool, UiToBevy};

pub(crate) fn show(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    let Some(settings) = &snapshot.sculpt else {
        ui.label(if snapshot.sculpt_received {
            "Sculpting is unavailable for this selection."
        } else {
            "Waiting for sculpt settings…"
        });
        return;
    };
    ui.heading("Sculpt brushes");
    presets::show(ui, snapshot, runtime, true, commands);
    let history = &snapshot.sculpt_history;
    ui.scope(|ui| {
        ui.horizontal_wrapped(|ui| {
            for tool in [
                SculptTool::Push,
                SculptTool::Pull,
                SculptTool::Grab,
                SculptTool::Smooth,
                SculptTool::Flatten,
                SculptTool::Inflate,
                SculptTool::Pinch,
                SculptTool::Crease,
            ] {
                if ui
                    .selectable_label(settings.tool == tool, format!("{tool:?}"))
                    .on_hover_text(description(tool))
                    .clicked()
                {
                    sculpt(commands, SculptCommand::SetTool { tool });
                }
            }
        });
        if let Some(radius) = slider(ui, settings.radius, 0.01..=10.0, "Radius (mesh units)") {
            sculpt(commands, SculptCommand::SetRadius { radius });
        }
        if let Some(strength) = slider(ui, settings.strength * 100.0, 0.0..=100.0, "Strength %") {
            sculpt(
                commands,
                SculptCommand::SetStrength {
                    strength: strength / 100.0,
                },
            );
        }
        if let Some(hardness) = slider(ui, settings.hardness * 100.0, 0.0..=100.0, "Hardness %") {
            sculpt(
                commands,
                SculptCommand::SetHardness {
                    hardness: hardness / 100.0,
                },
            );
        }
        egui::ComboBox::from_id_salt("sculpt_falloff")
            .selected_text(format!("{:?}", settings.falloff))
            .show_ui(ui, |ui| {
                for falloff in [
                    SculptFalloff::Linear,
                    SculptFalloff::Smooth,
                    SculptFalloff::Sharp,
                    SculptFalloff::Constant,
                    SculptFalloff::Sphere,
                ] {
                    if ui
                        .selectable_label(settings.falloff == falloff, format!("{falloff:?}"))
                        .clicked()
                    {
                        sculpt(commands, SculptCommand::SetFalloff { falloff });
                    }
                }
            });
        ui.add_enabled_ui(!history.active && settings.tool != SculptTool::Grab, |ui| {
            if let Some(amount) = slider(
                ui,
                settings.autosmooth * 100.0,
                0.0..=100.0,
                "Auto smoothing %",
            ) {
                sculpt(
                    commands,
                    SculptCommand::SetAutoSmooth {
                        amount: amount / 100.0,
                    },
                );
            }
        });
    });
    ui.small(format!("{}. Custom settings stay selected when switching tools. Object scale affects the brush footprint.", description(settings.tool)));
    ui.small(if settings.tool == SculptTool::Grab {
        "Grab stays continuous without post-dab smoothing."
    } else {
        "Auto smoothing follows each stamped dab. 0% preserves sharp detail."
    });
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        if button(
            ui,
            !history.active && history.undo_strokes > 0,
            "Undo sculpt stroke",
        ) {
            sculpt(commands, SculptCommand::Undo);
        }
        if button(
            ui,
            !history.active && history.redo_strokes > 0,
            "Redo sculpt stroke",
        ) {
            sculpt(commands, SculptCommand::Redo);
        }
    });
    ui.label(format!(
        "{} undo / {} redo",
        history.undo_strokes, history.redo_strokes
    ));
    ui.small("History lasts for this sculpt session. Escape rolls back an active stroke.");
    if let Some(notice) = &history.notice {
        ui.label(notice);
    }
    ui.small("F: radius · Shift+F: strength · Click or Enter confirms; Escape cancels adjustment.");
    ui.small("Middle-drag: orbit · Shift + middle-drag: pan · Scroll: zoom · Ctrl+Tab: leave sculpt mode");
}

fn description(tool: SculptTool) -> &'static str {
    match tool {
        SculptTool::Push => "Move along the hit surface normal",
        SculptTool::Pull => "Draw vertices toward the brush center",
        SculptTool::Grab => "Drag the surface along your stroke",
        SculptTool::Smooth => "Average neighboring vertex positions",
        SculptTool::Flatten => "Level the surface toward a plane",
        SculptTool::Inflate => "Expand along vertex normals",
        SculptTool::Pinch => "Gather the surface inward",
        SculptTool::Crease => "Carve a crease along the stroke",
    }
}
