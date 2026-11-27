use crate::hash::BinaryHashes;
use crate::memory_map::{MemoryError, MemoryMap};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Default maximum number of undoable transactions retained per document.
pub const DEFAULT_MAX_UNDO_DEPTH: usize = 256;

/// Single contiguous byte patch at `offset` inside a ROM buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytePatch {
    pub offset: usize,
    pub old_bytes: Vec<u8>,
    pub new_bytes: Vec<u8>,
}

/// Atomic edit transaction composed of one or more byte patches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditTransaction {
    pub id: u64,
    pub label: String,
    pub timestamp_ms: u64,
    pub patches: Vec<BytePatch>,
}

/// Snapshot of current undo/redo availability returned to callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoStatus {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_count: usize,
    pub redo_count: usize,
    pub last_label: Option<String>,
    pub revision: u64,
}

/// Outcome of executing an undo or redo step against a [`MemoryMap`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoStepOutcome {
    pub applied_label: String,
    pub changed_offsets: Vec<usize>,
    pub hashes: BinaryHashes,
    pub undo: UndoStatus,
}

/// In-place single-buffer Undo/Redo manager using compact byte patches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoHistory {
    undo_stack: VecDeque<EditTransaction>,
    redo_stack: Vec<EditTransaction>,
    next_id: u64,
    revision: u64,
    max_depth: usize,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_MAX_UNDO_DEPTH)
    }
}

impl UndoHistory {
    pub fn with_capacity(max_depth: usize) -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(max_depth.min(256)),
            redo_stack: Vec::new(),
            next_id: 1,
            revision: 0,
            max_depth: max_depth.max(1),
        }
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.revision = 0;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn status(&self) -> UndoStatus {
        UndoStatus {
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            undo_count: self.undo_stack.len(),
            redo_count: self.redo_stack.len(),
            last_label: self.undo_stack.back().map(|t| t.label.clone()),
            revision: self.revision,
        }
    }

    /// Records a committed transaction whose patches have already been written to `MemoryMap`.
    /// Empty or no-op transactions (`old_bytes == new_bytes` for all patches) are ignored.
    pub fn push(&mut self, label: impl Into<String>, timestamp_ms: u64, patches: Vec<BytePatch>) {
        let effective: Vec<BytePatch> = patches
            .into_iter()
            .filter(|p| p.old_bytes != p.new_bytes && !p.new_bytes.is_empty())
            .collect();
        if effective.is_empty() {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        if self.undo_stack.len() >= self.max_depth {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(EditTransaction {
            id,
            label: label.into(),
            timestamp_ms,
            patches: effective,
        });
        self.redo_stack.clear();
        self.revision = self.revision.saturating_add(1);
    }

    /// Reverts the most recent transaction in-place and recomputes hashes once.
    pub fn undo(&mut self, mem: &mut MemoryMap) -> Result<UndoStepOutcome, MemoryError> {
        let mem_len = mem.len();
        let tx_ref = match self.undo_stack.back() {
            Some(tx) => tx,
            None => {
                return Err(MemoryError::OutOfBounds {
                    addr: 0,
                    size: mem_len,
                });
            }
        };
        for patch in tx_ref.patches.iter().rev() {
            let valid = patch
                .offset
                .checked_add(patch.old_bytes.len())
                .is_some_and(|end| end <= mem_len);
            if !valid {
                return Err(MemoryError::OutOfBounds {
                    addr: patch.offset,
                    size: patch.old_bytes.len(),
                });
            }
        }
        let tx = self.undo_stack.pop_back().expect("validated above");
        let mut changed_offsets = Vec::with_capacity(tx.patches.len());
        for patch in tx.patches.iter().rev() {
            mem.write_slice_deferred_hash(patch.offset, &patch.old_bytes)?;
            changed_offsets.push(patch.offset);
        }
        let hashes = mem.recompute_hashes().clone();
        let label = tx.label.clone();
        self.redo_stack.push(tx);
        self.revision = self.revision.saturating_add(1);
        Ok(UndoStepOutcome {
            applied_label: label,
            changed_offsets,
            hashes,
            undo: self.status(),
        })
    }

    /// Re-applies the most recently undone transaction in-place and recomputes hashes once.
    pub fn redo(&mut self, mem: &mut MemoryMap) -> Result<UndoStepOutcome, MemoryError> {
        let mem_len = mem.len();
        let tx_ref = match self.redo_stack.last() {
            Some(tx) => tx,
            None => {
                return Err(MemoryError::OutOfBounds {
                    addr: 0,
                    size: mem_len,
                });
            }
        };
        for patch in &tx_ref.patches {
            let valid = patch
                .offset
                .checked_add(patch.new_bytes.len())
                .is_some_and(|end| end <= mem_len);
            if !valid {
                return Err(MemoryError::OutOfBounds {
                    addr: patch.offset,
                    size: patch.new_bytes.len(),
                });
            }
        }
        let tx = self.redo_stack.pop().expect("validated above");
        let mut changed_offsets = Vec::with_capacity(tx.patches.len());
        for patch in &tx.patches {
            mem.write_slice_deferred_hash(patch.offset, &patch.new_bytes)?;
            changed_offsets.push(patch.offset);
        }
        let hashes = mem.recompute_hashes().clone();
        let label = tx.label.clone();
        self.undo_stack.push_back(tx);
        self.revision = self.revision.saturating_add(1);
        Ok(UndoStepOutcome {
            applied_label: label,
            changed_offsets,
            hashes,
            undo: self.status(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_roundtrip_preserves_hashes_and_truncates_redo_branch() {
        let initial = vec![0x10u8; 32];
        let mut mem = MemoryMap::new(initial.clone());
        let h0 = mem.hashes().clone();
        let mut hist = UndoHistory::with_capacity(8);

        let (o1, n1) = mem.write_slice_deferred_hash(4, &[0xAA, 0xBB]).unwrap();
        let h1 = mem.recompute_hashes().clone();
        hist.push(
            "Edit cell",
            100,
            vec![BytePatch {
                offset: 4,
                old_bytes: o1,
                new_bytes: n1,
            }],
        );
        assert_eq!(&mem.as_slice()[4..6], &[0xAA, 0xBB]);
        assert!(hist.status().can_undo);
        assert!(!hist.status().can_redo);

        let (o2, n2) = mem.write_slice_deferred_hash(8, &[0xCC]).unwrap();
        let h2 = mem.recompute_hashes().clone();
        hist.push(
            "Edit second",
            200,
            vec![BytePatch {
                offset: 8,
                old_bytes: o2,
                new_bytes: n2,
            }],
        );

        let u1 = hist.undo(&mut mem).unwrap();
        assert_eq!(u1.hashes, h1);
        assert_eq!(mem.as_slice()[8], 0x10);
        assert!(u1.undo.can_redo);

        let u0 = hist.undo(&mut mem).unwrap();
        assert_eq!(u0.hashes, h0);
        assert_eq!(mem.as_slice(), initial.as_slice());

        let r1 = hist.redo(&mut mem).unwrap();
        assert_eq!(r1.hashes, h1);
        assert_eq!(&mem.as_slice()[4..6], &[0xAA, 0xBB]);

        let r2 = hist.redo(&mut mem).unwrap();
        assert_eq!(r2.hashes, h2);
        assert_eq!(mem.as_slice()[8], 0xCC);

        // Undo once and apply a new edit: redo stack must clear.
        hist.undo(&mut mem).unwrap();
        let (o3, n3) = mem.write_slice_deferred_hash(12, &[0xDD]).unwrap();
        mem.recompute_hashes();
        hist.push(
            "Branch edit",
            300,
            vec![BytePatch {
                offset: 12,
                old_bytes: o3,
                new_bytes: n3,
            }],
        );
        assert!(!hist.status().can_redo);
        assert_eq!(hist.status().undo_count, 2);
    }

    #[test]
    fn undo_capacity_is_capped_and_noop_ignored() {
        let mut mem = MemoryMap::new(vec![0u8; 16]);
        let mut hist = UndoHistory::with_capacity(2);
        // No-op push is ignored
        hist.push(
            "noop",
            1,
            vec![BytePatch {
                offset: 0,
                old_bytes: vec![0],
                new_bytes: vec![0],
            }],
        );
        assert_eq!(hist.status().undo_count, 0);

        for i in 1u8..=4 {
            let (old_bytes, new_bytes) = mem.write_slice_deferred_hash(0, &[i]).unwrap();
            hist.push(
                format!("step {i}"),
                i as u64,
                vec![BytePatch {
                    offset: 0,
                    old_bytes,
                    new_bytes,
                }],
            );
        }
        assert_eq!(hist.status().undo_count, 2);
        assert_eq!(hist.status().last_label.as_deref(), Some("step 4"));
    }
}
