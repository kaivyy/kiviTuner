use kivi_core::memory_map::MemoryMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ProjectError {
    #[error("Commit ID {0} not found in project history")]
    CommitNotFound(usize),
    #[error("Address 0x{addr:X} exceeds ROM length 0x{len:X}")]
    OutOfBounds { addr: usize, len: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellDelta {
    pub address: usize,
    pub old_raw: u8,
    pub new_raw: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub id: usize,
    pub message: String,
    pub timestamp: u64,
    pub deltas: Vec<CellDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSession {
    pub original_rom: Vec<u8>,
    pub current_rom: Vec<u8>,
    pub history: Vec<Commit>,
}

/// Compute byte-level deltas between two byte buffers.
pub fn compute_diff(before: &[u8], after: &[u8]) -> Vec<CellDelta> {
    let len = before.len().min(after.len());
    let mut deltas = Vec::new();
    for i in 0..len {
        if before[i] != after[i] {
            deltas.push(CellDelta {
                address: i,
                old_raw: before[i],
                new_raw: after[i],
            });
        }
    }
    deltas
}

impl ProjectSession {
    pub fn new(rom: Vec<u8>) -> Self {
        let current = rom.clone();
        Self {
            original_rom: rom,
            current_rom: current,
            history: Vec::new(),
        }
    }

    /// Construct a `ProjectSession` from a `MemoryMap`.
    pub fn from_memory_map(map: &MemoryMap) -> Self {
        Self::new(map.as_slice().to_vec())
    }

    /// Convert current ROM state into a `MemoryMap`.
    pub fn to_memory_map(&self) -> MemoryMap {
        MemoryMap::new(self.current_rom.clone())
    }

    /// Convert original ROM state into a `MemoryMap`.
    pub fn original_memory_map(&self) -> MemoryMap {
        MemoryMap::new(self.original_rom.clone())
    }

    /// Reconstruct the ROM snapshot at a given `commit_id`.
    /// `commit_id = 0` corresponds to the original ROM.
    pub fn rom_at_commit(&self, commit_id: usize) -> Result<Vec<u8>, ProjectError> {
        if commit_id == 0 {
            return Ok(self.original_rom.clone());
        }

        let target_idx = self
            .history
            .iter()
            .position(|c| c.id == commit_id)
            .ok_or(ProjectError::CommitNotFound(commit_id))?;

        let mut rom = self.original_rom.clone();
        for commit in &self.history[0..=target_idx] {
            for delta in &commit.deltas {
                if delta.address < rom.len() {
                    rom[delta.address] = delta.new_raw;
                }
            }
        }

        Ok(rom)
    }

    /// Return the ROM state as of the most recent commit, or `original_rom` if no commits.
    pub fn last_committed_rom(&self) -> Vec<u8> {
        if let Some(last_commit) = self.history.last() {
            self.rom_at_commit(last_commit.id)
                .unwrap_or_else(|_| self.original_rom.clone())
        } else {
            self.original_rom.clone()
        }
    }

    /// Commit the current uncommitted changes with a message and timestamp.
    /// Returns the assigned commit ID.
    pub fn commit(&mut self, message: &str, timestamp: u64) -> usize {
        let base_rom = self.last_committed_rom();
        let deltas = compute_diff(&base_rom, &self.current_rom);

        let commit_id = self.history.len() + 1;
        self.history.push(Commit {
            id: commit_id,
            message: message.to_string(),
            timestamp,
            deltas,
        });
        commit_id
    }

    /// Alias for `commit`.
    pub fn commit_changes(&mut self, message: &str, timestamp: u64) -> usize {
        self.commit(message, timestamp)
    }

    /// Roll back `current_rom` to the exact binary state at `commit_id`.
    /// `commit_id = 0` rolls back to the original baseline ROM.
    pub fn rollback(&mut self, commit_id: usize) -> Result<(), ProjectError> {
        let target_rom = self.rom_at_commit(commit_id)?;
        self.current_rom = target_rom;
        Ok(())
    }

    /// Roll back `current_rom` to the original ROM.
    pub fn rollback_to_original(&mut self) {
        self.current_rom = self.original_rom.clone();
    }

    /// Compute deltas between the latest committed state and `current_rom`.
    pub fn diff_uncommitted(&self) -> Vec<CellDelta> {
        let base_rom = self.last_committed_rom();
        compute_diff(&base_rom, &self.current_rom)
    }

    /// Compute deltas between `original_rom` and `current_rom`.
    pub fn diff_original(&self) -> Vec<CellDelta> {
        compute_diff(&self.original_rom, &self.current_rom)
    }

    /// Default diff: uncommitted changes against the latest commit.
    pub fn diff(&self) -> Vec<CellDelta> {
        self.diff_uncommitted()
    }

    /// Compute deltas between any two commits in the session.
    pub fn diff_commits(
        &self,
        from_id: usize,
        to_id: usize,
    ) -> Result<Vec<CellDelta>, ProjectError> {
        let from_rom = self.rom_at_commit(from_id)?;
        let to_rom = self.rom_at_commit(to_id)?;
        Ok(compute_diff(&from_rom, &to_rom))
    }
}

impl From<&MemoryMap> for ProjectSession {
    fn from(map: &MemoryMap) -> Self {
        Self::from_memory_map(map)
    }
}

impl From<MemoryMap> for ProjectSession {
    fn from(map: MemoryMap) -> Self {
        Self::from_memory_map(&map)
    }
}

/// Standalone helper to commit uncommitted changes on a project session.
pub fn commit_changes(session: &mut ProjectSession, message: &str, timestamp: u64) -> usize {
    session.commit(message, timestamp)
}

/// Standalone helper to rollback a project session to a specific commit.
pub fn rollback(session: &mut ProjectSession, commit_id: usize) -> Result<(), ProjectError> {
    session.rollback(commit_id)
}
