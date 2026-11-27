//! Kernel-session contract — what a running RAM kernel can be asked to do.
//!
//! ## Why this is separate from [`crate::device`]
//!
//! A device failure means the cable is gone. A session failure means the ECU
//! rejected a step while the cable is fine, and the recovery is different: the
//! kernel may still be resident in RAM and must be brought back to the stock OS
//! before the ECU can be left alone.
//!
//! [`KernelSession`] decouples RAM-resident kernel commands (probe, read,
//! erase, write, cleanup) from the low-level [`crate::device::Device`] transport.
//!
//! ## The cleanup guarantee
//!
//! `cleanup` must be idempotent and must succeed on every teardown path. Here
//! it is enforced by [`crate::flash_engine::CleanupGuard`], whose `Drop` calls
//! it even on early returns or panics.

use crate::j2534_types::ProtocolId;

/// What a kernel session was started to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KernelOperation {
    Read,
    Write,
    /// Read the image, modify, write back.
    ReadModifyWrite,
}

/// Where a session is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionPhase {
    /// Nothing running; the ECU is on its stock OS.
    Idle,
    /// The kernel is resident and responding.
    KernelRunning,
    /// The kernel has been asked to shut down and hand back to stock.
    RestoringStockOs,
    /// The kernel did not confirm the restore. The ECU must not be left here.
    RestoreFailed,
}

/// Session-layer failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelError {
    /// An operation arrived before `start`.
    KernelNotRunning,
    /// The ECU refused the kernel upload.
    UploadRejected,
    /// The kernel accepted the upload but did not start.
    KernelDidNotStart,
    /// The requested address was never scripted (test doubles only).
    NoDataForAddress(u32),
    /// A block came back the wrong size.
    LengthMismatch { expected: usize, got: usize },
    /// The ECU reported a negative response code.
    NegativeResponse(u8),
    /// The restore to stock OS failed. This is the dangerous one.
    RestoreFailed(String),
}

impl std::fmt::Display for KernelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KernelError::KernelNotRunning => f.write_str("no kernel is running"),
            KernelError::UploadRejected => f.write_str("the ECU rejected the kernel upload"),
            KernelError::KernelDidNotStart => f.write_str("the kernel did not start"),
            KernelError::NoDataForAddress(a) => write!(f, "no data scripted for address {a:#010x}"),
            KernelError::LengthMismatch { expected, got } => {
                write!(f, "expected {expected} bytes, got {got}")
            }
            KernelError::NegativeResponse(nrc) => write!(f, "negative response code {nrc:#04x}"),
            KernelError::RestoreFailed(m) => write!(f, "failed to restore stock OS: {m}"),
        }
    }
}

impl std::error::Error for KernelError {}

/// The contract a running kernel must satisfy.
pub trait KernelSession {
    fn phase(&self) -> SessionPhase;
    fn operation(&self) -> Option<KernelOperation>;
    fn is_kernel_running(&self) -> bool;

    /// Upload the kernel and wait for it to answer.
    fn start(&mut self, operation: KernelOperation) -> Result<(), KernelError>;
    /// Read `length` bytes from `address`.
    fn read_memory_block(&mut self, address: u32, length: usize) -> Result<Vec<u8>, KernelError>;
    /// Erase `length` bytes starting at `address`.
    fn erase_range(&mut self, address: u32, length: usize) -> Result<(), KernelError>;
    /// Write `data` at `address`.
    fn write_range(&mut self, address: u32, data: &[u8]) -> Result<(), KernelError>;
    /// Compare the ECU's contents at `address` against `expected`.
    fn verify_image(&mut self, address: u32, expected: &[u8]) -> Result<bool, KernelError>;
    /// Every mismatch as `(address, chip_byte, expected_byte)`.
    fn compare_image(
        &mut self,
        address: u32,
        expected: &[u8],
    ) -> Result<Vec<(u32, u8, u8)>, KernelError>;
    /// Stop the kernel and hand control back to the stock OS.
    ///
    /// Must be idempotent and must be called on every teardown path.
    fn cleanup(&mut self) -> Result<(), KernelError>;
    /// Reset the ECU's kernel watchdog so it does not reboot mid-transfer.
    fn keep_alive(&mut self) -> Result<(), KernelError>;
    /// The image size this session was configured for.
    fn image_size(&self) -> usize;
    /// The protocol the kernel speaks, when known.
    fn protocol(&self) -> Option<ProtocolId>;
    /// Number of completed `cleanup` invocations recorded by this session.
    fn cleanup_count(&self) -> usize {
        0
    }
}

/// A deterministic kernel session for tests.
///
/// Holds a sparse map of `address -> bytes` that `write_range` fills and
/// `read_memory_block` / `compare_image` consult. That makes a full
/// write-then-verify round trip expressible without hardware.
#[cfg(any(test, feature = "test-doubles"))]
#[derive(Debug, Clone)]
pub struct MockKernelSession {
    phase: SessionPhase,
    operation: Option<KernelOperation>,
    image_size: usize,
    protocol: Option<ProtocolId>,
    /// Sparse contents, keyed by start address.
    blocks: Vec<(u32, Vec<u8>)>,
    erased: Vec<(u32, usize)>,
    written: Vec<(u32, Vec<u8>)>,
    cleanup_count: usize,
    keep_alive_count: usize,
    start_failure: Option<KernelError>,
}

#[cfg(any(test, feature = "test-doubles"))]
impl MockKernelSession {
    pub fn new(image_size: usize) -> Self {
        Self {
            phase: SessionPhase::Idle,
            operation: None,
            image_size,
            protocol: Some(ProtocolId::Iso15765),
            blocks: Vec::new(),
            erased: Vec::new(),
            written: Vec::new(),
            cleanup_count: 0,
            keep_alive_count: 0,
            start_failure: None,
        }
    }

    /// Force a phase, for setting up a scenario.
    pub fn set_phase(&mut self, phase: SessionPhase) {
        self.phase = phase;
    }

    /// Make the next `start` fail with `err`.
    pub fn fail_start_with(&mut self, err: KernelError) {
        self.start_failure = Some(err);
    }

    /// Allow a retry after a scripted failure.
    pub fn clear_start_failure(&mut self) {
        self.start_failure = None;
    }

    /// Seed what `read_memory_block` will return for `address`.
    pub fn set_block_data(&mut self, address: u32, data: Vec<u8>) {
        self.blocks.retain(|(a, _)| *a != address);
        self.blocks.push((address, data));
    }

    pub fn erased_ranges(&self) -> Vec<(u32, usize)> {
        self.erased.clone()
    }

    pub fn written_bytes(&self) -> Vec<(u32, Vec<u8>)> {
        self.written.clone()
    }

    pub fn cleanup_count(&self) -> usize {
        self.cleanup_count
    }

    pub fn keep_alive_count(&self) -> usize {
        self.keep_alive_count
    }

    fn lookup(&self, address: u32) -> Option<&Vec<u8>> {
        self.blocks
            .iter()
            .find(|(a, _)| *a == address)
            .map(|(_, d)| d)
    }

    /// Materialise `[address, address + length)` from the sparse blocks.
    ///
    /// `compare_image` works over a whole range, which in a real session is one
    /// verify command handled inside the kernel — not a block-at-a-time read.
    /// The mock therefore has to stitch blocks together. Bytes covered by no
    /// block are reported as missing rather than assumed zero, so a test cannot
    /// accidentally pass against data it never supplied.
    fn assemble_region(&self, address: u32, length: usize) -> Option<Vec<u8>> {
        let mut out = vec![0u8; length];
        let mut covered = vec![false; length];
        for (start, data) in &self.blocks {
            let block_start = *start as u64;
            let block_end = block_start + data.len() as u64;
            let want_start = address as u64;
            let want_end = want_start + length as u64;
            let lo = block_start.max(want_start);
            let hi = block_end.min(want_end);
            if lo >= hi {
                continue;
            }
            let src = (lo - block_start) as usize;
            let dst = (lo - want_start) as usize;
            let n = (hi - lo) as usize;
            out[dst..dst + n].copy_from_slice(&data[src..src + n]);
            for c in covered.iter_mut().skip(dst).take(n) {
                *c = true;
            }
        }
        if covered.iter().all(|c| *c) {
            Some(out)
        } else {
            None
        }
    }
}

#[cfg(any(test, feature = "test-doubles"))]
impl KernelSession for MockKernelSession {
    fn phase(&self) -> SessionPhase {
        self.phase
    }

    fn operation(&self) -> Option<KernelOperation> {
        self.operation
    }

    fn is_kernel_running(&self) -> bool {
        matches!(self.phase, SessionPhase::KernelRunning)
    }

    fn start(&mut self, operation: KernelOperation) -> Result<(), KernelError> {
        if let Some(err) = self.start_failure.clone() {
            // A failed start must leave the session recoverable, not wedged.
            self.phase = SessionPhase::Idle;
            return Err(err);
        }
        self.phase = SessionPhase::KernelRunning;
        self.operation = Some(operation);
        Ok(())
    }

    fn read_memory_block(&mut self, address: u32, length: usize) -> Result<Vec<u8>, KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        let data = self
            .lookup(address)
            .ok_or(KernelError::NoDataForAddress(address))?;
        if data.len() != length {
            return Err(KernelError::LengthMismatch {
                expected: length,
                got: data.len(),
            });
        }
        Ok(data.clone())
    }

    fn erase_range(&mut self, address: u32, length: usize) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        self.erased.push((address, length));
        Ok(())
    }

    fn write_range(&mut self, address: u32, data: &[u8]) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        self.written.push((address, data.to_vec()));
        self.set_block_data(address, data.to_vec());
        Ok(())
    }

    fn verify_image(&mut self, address: u32, expected: &[u8]) -> Result<bool, KernelError> {
        Ok(self.compare_image(address, expected)?.is_empty())
    }

    fn compare_image(
        &mut self,
        address: u32,
        expected: &[u8],
    ) -> Result<Vec<(u32, u8, u8)>, KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        let actual = self
            .assemble_region(address, expected.len())
            .ok_or(KernelError::NoDataForAddress(address))?;
        let mut mismatches = Vec::new();
        for (i, &want) in expected.iter().enumerate() {
            let got = actual[i];
            if got != want {
                mismatches.push((address + i as u32, got, want));
            }
        }
        Ok(mismatches)
    }

    fn cleanup(&mut self) -> Result<(), KernelError> {
        // Idempotent: teardown runs on paths that already failed.
        self.phase = SessionPhase::Idle;
        self.operation = None;
        self.cleanup_count += 1;
        Ok(())
    }

    fn keep_alive(&mut self) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        self.keep_alive_count += 1;
        Ok(())
    }

    fn image_size(&self) -> usize {
        self.image_size
    }

    fn protocol(&self) -> Option<ProtocolId> {
        self.protocol
    }

    fn cleanup_count(&self) -> usize {
        self.cleanup_count
    }
}

impl<T: KernelSession + ?Sized> KernelSession for Box<T> {
    fn phase(&self) -> SessionPhase {
        (**self).phase()
    }
    fn operation(&self) -> Option<KernelOperation> {
        (**self).operation()
    }
    fn is_kernel_running(&self) -> bool {
        (**self).is_kernel_running()
    }
    fn start(&mut self, operation: KernelOperation) -> Result<(), KernelError> {
        (**self).start(operation)
    }
    fn read_memory_block(&mut self, address: u32, length: usize) -> Result<Vec<u8>, KernelError> {
        (**self).read_memory_block(address, length)
    }
    fn erase_range(&mut self, address: u32, length: usize) -> Result<(), KernelError> {
        (**self).erase_range(address, length)
    }
    fn write_range(&mut self, address: u32, data: &[u8]) -> Result<(), KernelError> {
        (**self).write_range(address, data)
    }
    fn verify_image(&mut self, address: u32, expected: &[u8]) -> Result<bool, KernelError> {
        (**self).verify_image(address, expected)
    }
    fn compare_image(
        &mut self,
        address: u32,
        expected: &[u8],
    ) -> Result<Vec<(u32, u8, u8)>, KernelError> {
        (**self).compare_image(address, expected)
    }
    fn cleanup(&mut self) -> Result<(), KernelError> {
        (**self).cleanup()
    }
    fn keep_alive(&mut self) -> Result<(), KernelError> {
        (**self).keep_alive()
    }
    fn image_size(&self) -> usize {
        (**self).image_size()
    }
    fn protocol(&self) -> Option<ProtocolId> {
        (**self).protocol()
    }
    fn cleanup_count(&self) -> usize {
        (**self).cleanup_count()
    }
}
