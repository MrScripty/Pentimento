use crate::{
    controls::button,
    egui,
    state::{EguiUiRuntime, EguiUiSnapshot},
};
use pentimento_ipc::{ProjectCommand, UiToBevy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    New,
    Save,
    Open,
}
impl Operation {
    fn receipt_slot(self) -> usize {
        match self {
            Self::New => 0,
            Self::Save => 1,
            Self::Open => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Save => "Save",
            Self::Open => "Open",
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct ProjectDialog {
    operation: Operation,
    generation: String,
    path: String,
    pending: bool,
    receipt_sequence: u64,
    error: Option<String>,
    first_frame: bool,
}
impl ProjectDialog {
    pub(crate) fn new(operation: Operation, snapshot: &EguiUiSnapshot) -> Self {
        Self {
            operation,
            generation: snapshot.project.generation.clone(),
            path: snapshot.project.path.clone().unwrap_or_default(),
            pending: false,
            receipt_sequence: 0,
            error: None,
            first_frame: true,
        }
    }
    fn submit(&mut self, snapshot: &EguiUiSnapshot, commands: &mut Vec<UiToBevy>) {
        if self.pending
            || !snapshot.project.available
            || snapshot.project.active
            || (self.operation != Operation::New && self.path.trim().is_empty())
        {
            return;
        }
        self.pending = true;
        self.receipt_sequence = snapshot.project_receipt.as_ref().map_or(0, |r| r.sequence);
        self.error = None;
        commands.push(UiToBevy::ProjectCommand(match self.operation {
            Operation::New => ProjectCommand::New {
                expected_generation: self.generation.clone(),
                confirm_discard: true,
            },
            Operation::Save => ProjectCommand::Save {
                path: self.path.trim().into(),
            },
            Operation::Open => ProjectCommand::Open {
                path: self.path.trim().into(),
            },
        }));
    }
}
pub(crate) fn toolbar(
    ui: &mut egui::Ui,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    let enabled =
        snapshot.project.available && !snapshot.project.active && runtime.project_dialog.is_none();
    ui.menu_button("File", |ui| {
        if button(ui, enabled, "Save project") {
            let mut dialog = ProjectDialog::new(Operation::Save, snapshot);
            if snapshot.project.path.is_some() && !snapshot.project.blocked {
                dialog.submit(snapshot, commands);
            }
            runtime.project_dialog = Some(dialog);
            ui.close();
        }
        for (label, operation) in [
            ("New project…", Operation::New),
            ("Open project…", Operation::Open),
            ("Save As…", Operation::Save),
        ] {
            if button(ui, enabled, label) {
                runtime.project_dialog = Some(ProjectDialog::new(operation, snapshot));
                ui.close();
            }
        }
    });
}
pub(crate) fn show(
    ctx: &egui::Context,
    snapshot: &EguiUiSnapshot,
    runtime: &mut EguiUiRuntime,
    commands: &mut Vec<UiToBevy>,
) {
    let Some(dialog) = runtime.project_dialog.as_mut() else {
        return;
    };
    if dialog.pending {
        if let Some(receipt) = &snapshot.project_receipts[dialog.operation.receipt_slot()] {
            if receipt.sequence > dialog.receipt_sequence
                && receipt.operation == dialog.operation.label()
            {
                if receipt.success {
                    runtime.project_dialog = None;
                    return;
                }
                dialog.pending = false;
                dialog.error = Some(receipt.message.clone());
            }
        }
    }
    let mut close = false;
    egui::Modal::new(egui::Id::new("project_dialog")).show(ctx,|ui| {
        ui.set_width(430.0);
        ui.heading(format!("{} project",dialog.operation.label()));
        ui.label(match dialog.operation {
            Operation::New=>"Create an empty project? Unsaved changes and all local Undo/Redo history will be discarded. Existing files remain on disk.",
            Operation::Save=>"Save editable geometry and paint layers to a local .pentimento.json file.",
            Operation::Open=>"Open a local .pentimento.json file and replace this document. Undo history starts fresh; live projection opens paused.",
        });
        if dialog.operation!=Operation::New {
            ui.label("Absolute local file path");
            let response=ui.add_enabled(!dialog.pending,egui::TextEdit::singleline(&mut dialog.path));
            if dialog.first_frame {response.request_focus();}
        }
        if let Some(error)=&dialog.error {ui.colored_label(egui::Color32::LIGHT_RED,error);}
        if let Some(notice)=&snapshot.project.notice {ui.label(notice);}
        ui.horizontal(|ui| {
            let cancel=ui.add_enabled(!dialog.pending,egui::Button::new("Cancel"));
            if dialog.first_frame && dialog.operation==Operation::New {cancel.request_focus();}
            close|=cancel.clicked();
            let enabled=!dialog.pending && snapshot.project.available && !snapshot.project.active && (dialog.operation==Operation::New || !dialog.path.trim().is_empty());
            if button(ui,enabled,if dialog.pending {"Working…"} else if dialog.operation==Operation::New {"Discard and create new"} else {dialog.operation.label()}) {dialog.submit(snapshot,commands);}
        });
        if !dialog.pending && ctx.input(|i|i.key_pressed(egui::Key::Escape)) {close=true;}
        dialog.first_frame=false;
    });
    if close {
        runtime.project_dialog = None;
    }
}
