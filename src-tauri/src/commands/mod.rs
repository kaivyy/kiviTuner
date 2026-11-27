use crate::datalog_manager::DatalogManager;
use crate::telemetry_manager::TelemetryManager;
use kivi_core::memory_map::MemoryMap;
use kivi_core::undo_history::UndoHistory;
use kivi_defs::model::MapDef;
use std::sync::{Mutex, MutexGuard};

pub const MAX_FILE_READ_BYTES: u64 = kivi_core::ingest::MAX_BINARY_SIZE as u64;

/// Acquire a mutex or surface an explicit error if a prior panic poisoned it.
pub fn lock_or_err<'a, T>(mutex: &'a Mutex<T>, name: &str) -> Result<MutexGuard<'a, T>, String> {
    mutex.lock().map_err(|_| {
        format!("Internal state lock '{name}' was poisoned after a previous error")
    })
}

/// Read a regular file with pre-read metadata bounds and a hard `Read::take` cap.
pub fn read_bounded_file_bytes(path: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file =
        std::fs::File::open(path).map_err(|e| format!("Failed to open file {}: {}", path, e))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("Failed to inspect file {}: {}", path, e))?;
    if !meta.is_file() {
        return Err(format!("Path {} is not a regular file", path));
    }
    if meta.len() > max_bytes {
        return Err(format!(
            "File {} size ({} bytes) exceeds limit ({} bytes)",
            path,
            meta.len(),
            max_bytes
        ));
    }
    let cap = usize::try_from(meta.len().min(16 * 1024 * 1024)).unwrap_or(0);
    let mut buf = Vec::with_capacity(cap);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| format!("Failed to read file {}: {}", path, e))?;
    if (buf.len() as u64) > max_bytes {
        return Err(format!(
            "File {} exceeds maximum allowed size ({} bytes)",
            path, max_bytes
        ));
    }
    Ok(buf)
}

/// Read a bounded UTF-8 text file safely without unbounded allocation.
pub fn read_bounded_file_string(path: &str, max_bytes: u64) -> Result<String, String> {
    let bytes = read_bounded_file_bytes(path, max_bytes)?;
    String::from_utf8(bytes).map_err(|e| format!("File {} is not valid UTF-8: {}", path, e))
}

/// Write bytes atomically via a same-directory temporary file, `sync_all`, and rename.
pub fn write_file_atomic(path: &str, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TMP_SEQ: AtomicU64 = AtomicU64::new(1);

    let target = std::path::Path::new(path);
    let parent = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
    };
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("rom.bin");
    let seq = NEXT_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), seq));

    let write_res = (|| -> std::io::Result<()> {
        let mut tmp = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        tmp.write_all(bytes)?;
        tmp.sync_all()?;
        drop(tmp);
        std::fs::rename(&tmp_path, target)?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("Failed to write file {}: {}", path, err));
    }
    Ok(())
}

pub struct AppState {
    pub memory: Mutex<Option<MemoryMap>>,
    pub original_memory: Mutex<Option<MemoryMap>>,
    pub maps: Mutex<Vec<MapDef>>,
    pub undo: Mutex<UndoHistory>,
    pub documents: Mutex<window_io::RomDocumentStore>,
    pub telemetry: TelemetryManager,
    pub datalog: DatalogManager,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            memory: Mutex::new(None),
            original_memory: Mutex::new(None),
            maps: Mutex::new(Vec::new()),
            undo: Mutex::new(UndoHistory::default()),
            documents: Mutex::new(window_io::RomDocumentStore::default()),
            telemetry: TelemetryManager::default(),
            datalog: DatalogManager::default(),
        }
    }
}

pub mod analysis;
pub mod definitions;
pub mod ecu_tools;
pub mod formats;
pub mod rom;
pub mod scanner;
pub mod telemetry;
pub mod window_io;

pub use analysis::*;
pub use definitions::*;
pub use ecu_tools::*;
pub use formats::*;
pub use rom::*;
pub use scanner::*;
pub use telemetry::*;
pub use window_io::*;
