//! Safe ROM binary patch inspection and application for `<XDFPATCH>` entries.

use serde::{Deserialize, Serialize};

use crate::model::PatchDef;

/// State of a multi-entry binary patch relative to a ROM image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchStatus {
    Original,
    Applied,
    Conflict { entry_index: usize, address: usize },
    OutOfBounds { entry_index: usize, address: usize },
}

/// Inspects whether `patch` is pristine (`Original`), `Applied`, out-of-bounds, or in `Conflict`.
pub fn inspect_patch_status(rom: &[u8], patch: &PatchDef) -> PatchStatus {
    if patch.entries.is_empty() {
        return PatchStatus::Original;
    }

    for (idx, entry) in patch.entries.iter().enumerate() {
        let patch_end = entry.address.checked_add(entry.patch_data.len());
        let base_end = entry.address.checked_add(entry.base_data.len());
        if patch_end.is_none_or(|e| e > rom.len()) || base_end.is_none_or(|e| e > rom.len()) {
            return PatchStatus::OutOfBounds {
                entry_index: idx,
                address: entry.address,
            };
        }
    }

    let all_applied = patch.entries.iter().all(|e| {
        !e.patch_data.is_empty() && rom[e.address..e.address + e.patch_data.len()] == e.patch_data[..]
    });
    if all_applied {
        return PatchStatus::Applied;
    }

    for (idx, entry) in patch.entries.iter().enumerate() {
        if !entry.base_data.is_empty()
            && rom[entry.address..entry.address + entry.base_data.len()] != entry.base_data[..]
        {
            return PatchStatus::Conflict {
                entry_index: idx,
                address: entry.address,
            };
        }
    }

    PatchStatus::Original
}

/// Applies (`revert = false`) or reverts (`revert = true`) `patch` onto a copy of `rom`
/// after verifying byte expectations cleanly.
pub fn apply_patch_safe(rom: &[u8], patch: &PatchDef, revert: bool) -> Result<Vec<u8>, String> {
    if patch.entries.is_empty() {
        return Err(format!("Patch '{}' has no patch entries", patch.name));
    }

    let status = inspect_patch_status(rom, patch);
    match status {
        PatchStatus::OutOfBounds {
            entry_index,
            address,
        } => {
            return Err(format!(
                "Patch '{}' entry #{entry_index} at 0x{address:X} exceeds ROM length 0x{:X}",
                patch.name,
                rom.len()
            ));
        }
        PatchStatus::Conflict {
            entry_index,
            address,
        } => {
            return Err(format!(
                "Patch '{}' entry #{entry_index} at 0x{address:X} does not match expected stock or patched bytes",
                patch.name
            ));
        }
        PatchStatus::Applied if !revert => return Ok(rom.to_vec()),
        PatchStatus::Original if revert => {
            if patch.entries.iter().any(|e| e.base_data.is_empty()) {
                return Err(format!(
                    "Patch '{}' cannot be reverted because stock base_data is missing",
                    patch.name
                ));
            }
            return Ok(rom.to_vec());
        }
        _ => {}
    }

    let mut out = rom.to_vec();
    if revert {
        for (idx, entry) in patch.entries.iter().enumerate() {
            if entry.base_data.is_empty() {
                return Err(format!(
                    "Patch '{}' entry #{idx} has no stock base_data to revert",
                    patch.name
                ));
            }
            out[entry.address..entry.address + entry.base_data.len()]
                .copy_from_slice(&entry.base_data);
        }
    } else {
        for entry in &patch.entries {
            out[entry.address..entry.address + entry.patch_data.len()]
                .copy_from_slice(&entry.patch_data);
        }
    }

    Ok(out)
}
