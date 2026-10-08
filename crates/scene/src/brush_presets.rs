//! One backend-owned, versioned device-local catalog for both brush modes.
//! Saved paint entries use the engine preset, including its pressure size range;
//! sculpt entries use the existing editor snapshot and explicit smoothing override.
use crate::{OutboundUiMessages, PaintingResource};
use bevy::prelude::*;
use pentimento_ipc::{
    BevyToUi, PaintBrushPresetInfo, PaintCommand, SculptBrushSettings, SculptCommand,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: usize = 64;
const FILE_LIMIT: u64 = 1024 * 1024;
#[cfg(test)]
#[path = "brush_preset_tests.rs"]
pub(crate) mod tests;

#[derive(Clone, Serialize, Deserialize)]
struct PaintEntry {
    id: u32,
    name: String,
    brush: painting::BrushPreset,
    color: [f32; 4],
    blend: pentimento_ipc::BlendMode,
}
#[derive(Clone, Serialize, Deserialize)]
struct SculptEntry {
    id: u32,
    name: String,
    settings: SculptBrushSettings,
    autosmooth_override: Option<f32>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Document {
    version: u32,
    paint: Vec<PaintEntry>,
    sculpt: Vec<SculptEntry>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            version: 1,
            paint: vec![],
            sculpt: vec![],
        }
    }
}

#[derive(Resource)]
struct Catalog {
    document: Document,
    path: Option<PathBuf>,
    original: Option<Vec<u8>>,
    blocked: bool,
    notice: Option<String>,
    selected_paint: Option<u32>,
    selected_sculpt: Option<u32>,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
}
fn unit(n: f32) -> bool {
    n.is_finite() && (0.0..=1.0).contains(&n)
}
fn valid_paint(p: &PaintEntry) -> bool {
    valid_name(&p.name)
        && p.id > 0
        && p.brush.base_size.is_finite()
        && (1.0..=512.0).contains(&p.brush.base_size)
        && p.brush.min_size.is_finite()
        && (0.0..=p.brush.max_size).contains(&p.brush.min_size)
        && p.brush.max_size.is_finite()
        && (1.0..=512.0).contains(&p.brush.max_size)
        && unit(p.brush.opacity)
        && unit(p.brush.hardness)
        && p.brush.spacing.is_finite()
        && (0.01..=1.0).contains(&p.brush.spacing)
        && p.color.iter().copied().all(unit)
        && p.brush.name.chars().count() <= 64
}
fn valid_sculpt(p: &SculptEntry) -> bool {
    #[cfg(feature = "sculpting")]
    if crate::sculpt_mode::preset_from_saved_settings(&p.settings, p.autosmooth_override).is_err() {
        return false;
    }
    valid_name(&p.name)
        && p.id > 0
        && p.settings.radius.is_finite()
        && (0.01..=10.0).contains(&p.settings.radius)
        && unit(p.settings.strength)
        && unit(p.settings.hardness)
        && unit(p.settings.autosmooth)
        && p.autosmooth_override.is_none_or(unit)
        && (p.settings.tool != pentimento_ipc::SculptTool::Grab || p.settings.autosmooth == 0.0)
        && (p.settings.tool == pentimento_ipc::SculptTool::Grab
            || p.autosmooth_override
                .is_none_or(|n| n == p.settings.autosmooth))
}
impl Document {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.paint.len() > LIMIT
            || self.sculpt.len() > LIMIT
            || !self.paint.iter().all(valid_paint)
            || !self.sculpt.iter().all(valid_sculpt)
        {
            return Err("The saved brush catalog has an unsupported version or invalid settings; it was left unchanged.".into());
        }
        for list in [
            self.paint
                .iter()
                .map(|p| (p.id, &p.name))
                .collect::<Vec<_>>(),
            self.sculpt.iter().map(|p| (p.id, &p.name)).collect(),
        ] {
            let mut ids = std::collections::HashSet::new();
            let mut names = std::collections::HashSet::new();
            if !list
                .into_iter()
                .all(|(id, name)| ids.insert(id) && names.insert(name))
            {
                return Err("The saved brush catalog contains duplicate names or identities; it was left unchanged.".into());
            }
        }
        Ok(())
    }
}
fn read_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read saved brushes: {e}")),
    };
    let mut bytes = vec![];
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read saved brushes: {e}"))?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(
            "Saved brushes exceed the 1 MiB catalog limit; the file was left unchanged.".into(),
        );
    }
    Ok(Some(bytes))
}
impl Catalog {
    fn load(path: Option<PathBuf>) -> Self {
        let mut result = Self {
            document: Document::default(),
            path,
            original: None,
            blocked: false,
            notice: None,
            selected_paint: None,
            selected_sculpt: None,
        };
        let loaded = match result.path.as_deref() {
            Some(path) => read_file(path),
            None => {
                Err("Saving custom brushes is unavailable in this renderer environment.".into())
            }
        };
        let parsed = loaded.and_then(|bytes| {
            let document = match &bytes {
                Some(bytes) => serde_json::from_slice::<Document>(bytes).map_err(|_| {
                    "Saved brushes could not be decoded; the file was left unchanged.".to_owned()
                })?,
                None => Document::default(),
            };
            document.validate()?;
            Ok((document, bytes))
        });
        match parsed {
            Ok((document, bytes)) => {
                result.document = document;
                result.original = bytes;
            }
            Err(error) => {
                result.blocked = true;
                result.notice = Some(error);
            }
        }
        result
    }
    fn persist(&mut self, document: Document) -> Result<(), String> {
        if self.blocked {
            return Err(self
                .notice
                .clone()
                .unwrap_or_else(|| "Saving brushes is unavailable.".into()));
        }
        document.validate()?;
        let path = self
            .path
            .as_ref()
            .ok_or("Saving custom brushes is unavailable.")?;
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|e| format!("Cannot encode saved brushes: {e}"))?;
        if bytes.len() as u64 > FILE_LIMIT {
            return Err("Saved brushes exceed the 1 MiB catalog limit.".into());
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create brush storage: {e}"))?;
        // Lock a stable sidecar, not the catalog inode replaced by rename.
        // Nonblocking acquisition keeps another editor's save off the render loop.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("json.lock"))
            .map_err(|e| format!("Cannot open brush storage lock: {e}"))?;
        lock.try_lock().map_err(|_| "Another editor is saving brushes, or storage locking is unavailable. Try saving again.".to_owned())?;
        if read_file(path)? != self.original {
            self.blocked = true;
            return Err("Saved brushes changed outside this editor. Restart to load them before saving again; the file was left unchanged.".into());
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(".brush-presets-{}-{nonce}.tmp", std::process::id()));
        let written = (|| -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temporary, path)?;
            Ok(())
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("Custom brush was not saved: {e}"));
        }
        self.document = document;
        self.original = Some(bytes);
        self.notice = Some("Custom brush saved on this computer.".into());
        Ok(())
    }
}

fn storage_path() -> Option<PathBuf> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    if let Some(path) = std::env::var_os("PENTIMENTO_BRUSH_PRESETS_PATH") {
        return Some(path.into());
    }
    let root = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
    };
    root.map(|p| p.join("pentimento/brush-presets.json"))
}
pub(crate) fn ensure(world: &mut World) {
    if !world.contains_resource::<Catalog>() {
        world.insert_resource(Catalog::load(storage_path()));
    }
}
pub(crate) fn active(world: &World) -> bool {
    let active = world
        .get_resource::<crate::PaintMode>()
        .is_some_and(|s| s.current_stroke.is_some())
        || world
            .get_resource::<PaintingResource>()
            .is_some_and(PaintingResource::has_active_stroke);
    #[cfg(feature = "sculpting")]
    {
        return active
            || world
                .get_resource::<crate::SculptState>()
                .is_some_and(|s| s.current_stroke_id.is_some())
            || world
                .get_resource::<crate::sculpt_mode::SculptingData>()
                .and_then(|s| s.pipeline.as_ref())
                .is_some_and(|p| p.is_stroke_active());
    }
    #[cfg(not(feature = "sculpting"))]
    active
}
pub(crate) fn message(world: &World) -> Option<BevyToUi> {
    let catalog = world.get_resource::<Catalog>()?;
    let selected_paint = world
        .get_resource::<PaintingResource>()
        .and_then(|current| {
            catalog
                .document
                .paint
                .iter()
                .find(|p| {
                    Some(p.id) == catalog.selected_paint
                        && p.brush == current.brush_preset
                        && p.color == current.brush_color
                        && p.blend == crate::brush_ui::paint_snapshot(current).blend_mode
                })
                .map(|p| p.id)
        });
    #[cfg(feature = "sculpting")]
    let selected_sculpt = world.get_resource::<crate::SculptState>().and_then(|s| {
        catalog
            .document
            .sculpt
            .iter()
            .find(|p| {
                Some(p.id) == catalog.selected_sculpt
                    && p.settings == crate::sculpt_mode::sculpt_snapshot(s)
                    && p.autosmooth_override == s.brush_autosmooth
            })
            .map(|p| p.id)
    });
    #[cfg(not(feature = "sculpting"))]
    let selected_sculpt = None;
    Some(BevyToUi::SavedBrushPresetsChanged {
        paint: catalog
            .document
            .paint
            .iter()
            .map(|p| PaintBrushPresetInfo {
                id: p.id,
                name: p.name.clone(),
            })
            .collect(),
        sculpt: catalog
            .document
            .sculpt
            .iter()
            .map(|p| PaintBrushPresetInfo {
                id: p.id,
                name: p.name.clone(),
            })
            .collect(),
        selected_paint,
        selected_sculpt,
        active: active(world),
        available: !catalog.blocked,
        notice: catalog.notice.clone(),
    })
}
pub(crate) fn reject(world: &mut World, error: impl Into<String>) {
    let message = error.into();
    if let Some(mut catalog) = world.get_resource_mut::<Catalog>() {
        catalog.notice = Some(message.clone());
    }
    if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
        outbound.send(BevyToUi::Error {
            code: "brush_preset_rejected".into(),
            message,
        });
    }
}
fn save(world: &mut World, name: &str, sculpt: bool) -> Result<(), String> {
    let name = name.trim();
    if !valid_name(name) {
        return Err("Use a brush name of 1–64 characters without control characters.".into());
    }
    let mut document = world.resource::<Catalog>().document.clone();
    let selected_id;
    if sculpt {
        #[cfg(feature = "sculpting")]
        {
            let state = world
                .get_resource::<crate::SculptState>()
                .ok_or("Sculpt brushes are unavailable.")?;
            let existing = document.sculpt.iter().position(|p| p.name == name);
            let id = if let Some(index) = existing {
                document.sculpt[index].id
            } else {
                if document.sculpt.len() >= LIMIT {
                    return Err("The 64 saved sculpt brush limit is reached. Reuse a name to replace a brush.".into());
                }
                document
                    .sculpt
                    .iter()
                    .map(|p| p.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("Sculpt preset identities exhausted.")?
            };
            let entry = SculptEntry {
                id,
                name: name.into(),
                settings: crate::sculpt_mode::sculpt_snapshot(state),
                autosmooth_override: state.brush_autosmooth,
            };
            selected_id = id;
            if let Some(index) = existing {
                document.sculpt[index] = entry;
            } else {
                document.sculpt.push(entry);
            }
        }
        #[cfg(not(feature = "sculpting"))]
        return Err("Sculpt brushes are unavailable in this build.".into());
    } else {
        let paint = world
            .get_resource::<PaintingResource>()
            .ok_or("Paint brushes are unavailable.")?;
        let existing = document.paint.iter().position(|p| p.name == name);
        let id = if let Some(index) = existing {
            document.paint[index].id
        } else {
            if document.paint.len() >= LIMIT {
                return Err(
                    "The 64 saved paint brush limit is reached. Reuse a name to replace a brush."
                        .into(),
                );
            }
            document
                .paint
                .iter()
                .map(|p| p.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Paint preset identities exhausted.")?
        };
        let entry = PaintEntry {
            id,
            name: name.into(),
            brush: paint.brush_preset.clone(),
            color: paint.brush_color,
            blend: crate::brush_ui::paint_snapshot(paint).blend_mode,
        };
        selected_id = id;
        if let Some(index) = existing {
            document.paint[index] = entry;
        } else {
            document.paint.push(entry);
        }
    }
    let mut catalog = world.resource_mut::<Catalog>();
    catalog.persist(document)?;
    if sculpt {
        catalog.selected_sculpt = Some(selected_id);
    } else {
        catalog.selected_paint = Some(selected_id);
    }
    Ok(())
}
fn select(world: &mut World, id: u32, sculpt: bool) -> Result<(), String> {
    if world.resource::<Catalog>().blocked {
        return Err(world
            .resource::<Catalog>()
            .notice
            .clone()
            .unwrap_or_else(|| "Saved brushes are unavailable.".into()));
    }
    if sculpt {
        let entry = world
            .resource::<Catalog>()
            .document
            .sculpt
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or("That saved sculpt brush is unavailable.")?;
        #[cfg(feature = "sculpting")]
        crate::sculpt_mode::restore_saved_brush(world, &entry.settings, entry.autosmooth_override)?;
        #[cfg(not(feature = "sculpting"))]
        {
            let _ = entry;
            return Err("Sculpt brushes are unavailable in this build.".into());
        }
    } else {
        let entry = world
            .resource::<Catalog>()
            .document
            .paint
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or("That saved paint brush is unavailable.")?;
        let mut paint = world
            .get_resource_mut::<PaintingResource>()
            .ok_or("Paint brushes are unavailable.")?;
        paint.set_brush_preset(entry.brush);
        paint.set_brush_color(entry.color);
        paint.set_blend_mode_ipc(entry.blend);
        drop(paint);
        // A recalled brush is ready to paint; retain consumed-press ownership.
        if let Some(mut mode) = world.get_resource_mut::<crate::PaintMode>() {
            mode.sample_color = false;
        }
    }
    let mut catalog = world.resource_mut::<Catalog>();
    if sculpt {
        catalog.selected_sculpt = Some(id);
    } else {
        catalog.selected_paint = Some(id);
    }
    catalog.notice = Some("Saved brush restored.".into());
    Ok(())
}
pub(crate) fn paint_command(world: &mut World, command: &PaintCommand) -> bool {
    let result = match command {
        PaintCommand::SaveBrushPreset { name } => Some((Some(name.as_str()), None)),
        PaintCommand::SelectSavedBrushPreset { preset_id } => Some((None, Some(*preset_id))),
        _ => None,
    };
    handle(world, result, false)
}
pub(crate) fn sculpt_command(world: &mut World, command: &SculptCommand) -> bool {
    let result = match command {
        SculptCommand::SaveBrushPreset { name } => Some((Some(name.as_str()), None)),
        SculptCommand::SelectSavedBrushPreset { preset_id } => Some((None, Some(*preset_id))),
        _ => None,
    };
    handle(world, result, true)
}
fn handle(world: &mut World, command: Option<(Option<&str>, Option<u32>)>, sculpt: bool) -> bool {
    let Some((name, id)) = command else {
        return false;
    };
    ensure(world);
    let result = if active(world) {
        Err("Finish or cancel the active stroke before saving or restoring a brush preset.".into())
    } else if let Some(name) = name {
        save(world, name, sculpt)
    } else {
        select(world, id.unwrap(), sculpt)
    };
    if let Err(error) = result {
        reject(world, error);
    }
    true
}
