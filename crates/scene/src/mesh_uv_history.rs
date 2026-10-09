//! Bounded local DirectUV pixel transactions. No geometry writes or replay packets.
use std::collections::VecDeque;

pub(crate) const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;
pub(crate) struct UvEntry {
    pub mesh_id: u32,
    pub dimensions: (u32, u32),
    pub before_bound: bool,
    pub after_bound: bool,
    pub before: Vec<[f32; 4]>,
    pub after: Vec<[f32; 4]>,
}
impl UvEntry {
    fn bytes(&self) -> usize {
        (self.before.len() + self.after.len()) * 16
    }
}
#[derive(Default)]
pub(crate) struct UvHistory {
    pub undo: VecDeque<UvEntry>,
    pub redo: VecDeque<UvEntry>,
    pub evicted_strokes: usize,
}
impl UvHistory {
    pub fn bytes(&self) -> usize {
        self.undo.iter().chain(&self.redo).map(UvEntry::bytes).sum()
    }
    pub fn reserve(&mut self, bytes: usize) -> bool {
        if bytes > MAX_HISTORY_BYTES {
            return false;
        }
        while self.bytes() > MAX_HISTORY_BYTES - bytes || self.undo.len() + self.redo.len() >= 128 {
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
            self.evicted_strokes += 1;
        }
        true
    }
    pub fn clear_mesh(&mut self, id: u32) {
        self.undo.retain(|e| e.mesh_id != id);
        self.redo.retain(|e| e.mesh_id != id);
    }
}
pub(crate) fn same_pixels(a: &[[f32; 4]], b: &[[f32; 4]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .flatten()
            .zip(b.iter().flatten())
            .all(|(a, b)| a.to_bits() == b.to_bits())
}
