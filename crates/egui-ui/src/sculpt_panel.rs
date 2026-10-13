use crate::{
    controls::{button, sculpt, slider},
    egui, presets,
    state::{EguiUiRuntime, EguiUiSnapshot},
};
use pentimento_ipc::{SculptCommand, SculptFalloff, SculptTool, UiToBevy};

pub(crate) fn toolbar(ui: &mut egui::Ui, snapshot: &EguiUiSnapshot, commands: &mut Vec<UiToBevy>) {
    let Some(settings) = &snapshot.sculpt else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        ui.strong(format!("{:?}", settings.tool));
        if let Some(radius) =
            crate::controls::toolbar_value(ui, settings.radius, 0.01..=10.0, "Radius (mesh units)")
        {
            sculpt(commands, SculptCommand::SetRadius { radius });
        }
        if let Some(strength) =
            crate::controls::toolbar_value(ui, settings.strength * 100.0, 0.0..=100.0, "Strength %")
        {
            sculpt(
                commands,
                SculptCommand::SetStrength {
                    strength: strength / 100.0,
                },
            );
        }
        if let Some(hardness) =
            crate::controls::toolbar_value(ui, settings.hardness * 100.0, 0.0..=100.0, "Hardness %")
        {
            sculpt(
                commands,
                SculptCommand::SetHardness {
                    hardness: hardness / 100.0,
                },
            );
        }
    });
    let history = &snapshot.sculpt_history;
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
        ui.label(format!(
            "{} undo / {} redo",
            history.undo_strokes, history.redo_strokes
        ));
        if history.active {
            ui.label("Stroke active · Escape to cancel");
        }
    });
}

pub(crate) fn shelf(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    if ui.available_width() >= 500.0 {
        ui.columns(2, |columns| {
            tool_tiles(&mut columns[0], snapshot, commands);
            presets::show(&mut columns[1], snapshot, runtime, true, commands);
        });
    } else {
        tool_tiles(ui, snapshot, commands);
        presets::show(ui, snapshot, runtime, true, commands);
    }
}

fn tool_tiles(ui: &mut egui::Ui, snapshot: &EguiUiSnapshot, commands: &mut Vec<UiToBevy>) {
    ui.strong("Sculpt brushes");
    if let Some(settings) = &snapshot.sculpt {
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
                if crate::controls::tile(ui, settings.tool == tool, &format!("{tool:?}"))
                    .on_hover_text(description(tool))
                    .clicked()
                {
                    sculpt(commands, SculptCommand::SetTool { tool });
                }
            }
        });
    }
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    _runtime: &mut EguiUiRuntime,
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
    ui.heading("Brush properties");
    ui.label(description(settings.tool));
    ui.separator();
    ui.strong("Falloff");
    ui.horizontal_wrapped(|ui| {
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
    ui.add_enabled_ui(
        !snapshot.sculpt_history.active && settings.tool != SculptTool::Grab,
        |ui| {
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
        },
    );
    ui.small(if settings.tool == SculptTool::Grab {
        "Grab stays continuous without post-dab smoothing."
    } else {
        "Auto smoothing follows each stamped dab. 0% preserves sharp detail."
    });
    ui.small("Custom settings stay selected when switching tools. Object scale affects the brush footprint.");
    ui.separator();
    ui.strong("Stroke history");
    ui.small("Undo and Redo restore committed strokes for this session. Escape rolls back an active stroke.");
    if let Some(notice) = &snapshot.sculpt_history.notice {
        ui.label(notice);
    }
    ui.separator();
    ui.strong("Navigation");
    ui.small("F: radius · Shift+F: strength · Click or Enter confirms; Escape cancels adjustment.");
    ui.small("Middle-drag: orbit · Shift+middle: pan · Scroll: zoom · Ctrl+Tab: leave sculpt mode");
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
