//! Flash orchestrator — the driver that was missing from the chain.
//!
//! ## What this module does
//!
//! It drives a [`crate::device::Device`] and a
//! [`crate::kernel_session::KernelSession`] through a complete read or write,
//! and guarantees the ECU gets its stock OS back on every exit path.
//!
//! ## The cleanup guarantee
//!
//! [`CleanupGuard`] holds a mutable borrow of the session and calls `cleanup`
//! from Rust's RAII `Drop` destructor so the ECU always exits programming mode cleanly on:
//!
//! - normal return,
//! - an early `?` return,
//! - a panic that unwinds,
//!
//! which are exactly the three cases that matter. The guard can be disarmed with
//! [`CleanupGuard::cleanup_now`] when cleanup already happened explicitly.
//!
//! ## What this module is not
//!
//! It does not decide *what* to flash. [`FlashPlan`] is supplied by the caller,
//! usually from [`crate::flash_writer`]'s differential planner, and the image
//! must already be the right length. All validation that can be done before
//! touching the device is done before the device is touched.

use crate::device::{Device, DeviceError};
use crate::kernel_session::{KernelError, KernelOperation, KernelSession};

/// One contiguous region to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashRange {
    pub address: u32,
    pub length: usize,
}

/// Which regions a flash will touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashPlan {
    /// Base address the plan is expressed against.
    pub base_address: u32,
    pub ranges: Vec<(u32, usize)>,
}

impl FlashPlan {
    /// A plan covering `[base, base + length)` in one range.
    pub fn full(base_address: u32, length: usize) -> Self {
        Self {
            base_address,
            ranges: vec![(base_address, length)],
        }
    }

    /// A plan touching only the given `(address, length)` pairs.
    pub fn differential(base_address: u32, _total: usize, changed: &[(u32, usize)]) -> Self {
        Self {
            base_address,
            ranges: changed.to_vec(),
        }
    }

    /// `true` when the plan covers one contiguous range starting at the base.
    pub fn is_full(&self) -> bool {
        self.ranges.len() == 1 && self.ranges[0].0 == self.base_address
    }

    /// Total bytes the plan will write (saturates on `usize` overflow).
    pub fn total_bytes(&self) -> usize {
        self.ranges
            .iter()
            .fold(0usize, |acc, (_, l)| acc.saturating_add(*l))
    }

    fn checked_total_bytes(&self) -> Result<usize, FlashError> {
        self.ranges
            .iter()
            .try_fold(0usize, |acc, (_, l)| acc.checked_add(*l))
            .ok_or(FlashError::AddressOverflow)
    }
}

fn validate_u32_range(start: u32, length: usize) -> Result<(), FlashError> {
    if length == 0 {
        return Ok(());
    }
    let last = u32::try_from(length - 1).map_err(|_| FlashError::AddressOverflow)?;
    start.checked_add(last).ok_or(FlashError::AddressOverflow)?;
    Ok(())
}

/// What went wrong during a flash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlashError {
    /// The device layer failed.
    Device(DeviceError),
    /// The kernel session failed.
    Kernel(KernelError),
    /// The image is shorter than the plan needs.
    ImageTooShort { needed: usize, got: usize },
    /// The image is longer than the plan allows.
    ImageTooLong { expected: usize, got: usize },
    /// A plan with no ranges.
    EmptyPlan,
    /// Verification found differences after writing.
    VerificationFailed { mismatch_count: usize },
    /// Two plan ranges cover the same byte.
    ///
    /// Caught before any device call: if two ranges overlap, the final contents
    /// depend on write order, which is not guaranteed — and each individual
    /// write would still report success.
    OverlappingRanges,
    /// A region or total transfer length overflows 32-bit ECU address space.
    AddressOverflow,
}

impl From<DeviceError> for FlashError {
    fn from(e: DeviceError) -> Self {
        FlashError::Device(e)
    }
}

impl From<KernelError> for FlashError {
    fn from(e: KernelError) -> Self {
        FlashError::Kernel(e)
    }
}

impl std::fmt::Display for FlashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlashError::Device(e) => write!(f, "device: {e}"),
            FlashError::Kernel(e) => write!(f, "kernel: {e}"),
            FlashError::ImageTooShort { needed, got } => {
                write!(f, "image is {got} bytes, the plan needs {needed}")
            }
            FlashError::ImageTooLong { expected, got } => {
                write!(f, "image is {got} bytes, the plan allows {expected}")
            }
            FlashError::EmptyPlan => f.write_str("the flash plan is empty"),
            FlashError::VerificationFailed { mismatch_count } => {
                write!(f, "verification found {mismatch_count} mismatched bytes")
            }
            FlashError::OverlappingRanges => f.write_str(
                "the flash plan contains overlapping ranges; the result would depend \
                 on write order",
            ),
            FlashError::AddressOverflow => {
                f.write_str("flash range overflows 32-bit ECU address space")
            }
        }
    }
}

impl std::error::Error for FlashError {}

/// Result of a successful dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpOutcome {
    pub base_address: u32,
    pub bytes: Vec<u8>,
    pub blocks_read: usize,
}

/// Result of a successful flash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashOutcome {
    pub verified: bool,
    pub bytes_written: usize,
    pub blocks_written: usize,
}

/// Restores the ECU's stock OS when dropped.
///
/// Holding `&mut S` means the session cannot be used elsewhere while the guard
/// is alive, which prevents a second `start` racing the cleanup.
pub struct CleanupGuard<'a, S: KernelSession + ?Sized> {
    session: &'a mut S,
    armed: bool,
}

impl<'a, S: KernelSession + ?Sized> CleanupGuard<'a, S> {
    pub fn new(session: &'a mut S) -> Self {
        Self {
            session,
            armed: true,
        }
    }

    /// Run cleanup now and disarm, so `Drop` does not run it a second time.
    pub fn cleanup_now(&mut self) -> Result<(), KernelError> {
        self.armed = false;
        self.session.cleanup()
    }
}

impl<S: KernelSession + ?Sized> Drop for CleanupGuard<'_, S> {
    fn drop(&mut self) {
        if self.armed {
            // Cleanup swallows its own error here: a panic is already unwinding
            // and a second panic would abort the process. The session records
            // the attempt via its own counter so callers can still observe it.
            let _ = self.session.cleanup();
        }
    }
}

/// Active phase reported by [`flash_ecu_with_progress`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashTransferStage {
    Erasing,
    Writing,
    Verifying,
}

/// Read `[address, address + length)` from the ECU into a flat image.
///
/// Blocks are requested in `block_size` chunks, reporting `(bytes_done, total_bytes, blocks_read)`
/// after each block. Cleanup is guaranteed.
pub fn dump_ecu_with_progress<D, S, F>(
    _device: &mut D,
    session: &mut S,
    address: u32,
    length: usize,
    block_size: usize,
    mut on_progress: F,
) -> Result<DumpOutcome, FlashError>
where
    D: Device + ?Sized,
    S: KernelSession + ?Sized,
    F: FnMut(usize, usize, usize),
{
    if block_size == 0 {
        return Err(FlashError::EmptyPlan);
    }
    validate_u32_range(address, length)?;

    let mut guard = CleanupGuard::new(session);
    guard.session.start(KernelOperation::Read)?;

    let mut bytes = Vec::with_capacity(length);
    let mut blocks_read = 0usize;
    let mut offset = 0usize;
    while offset < length {
        let want = (length - offset).min(block_size);
        let read_addr = address
            .checked_add(u32::try_from(offset).map_err(|_| FlashError::AddressOverflow)?)
            .ok_or(FlashError::AddressOverflow)?;
        let block = guard.session.read_memory_block(read_addr, want)?;
        if block.len() != want {
            return Err(FlashError::Kernel(KernelError::LengthMismatch {
                expected: want,
                got: block.len(),
            }));
        }
        bytes.extend_from_slice(&block);
        blocks_read += 1;
        offset += want;
        on_progress(offset, length, blocks_read);
    }

    guard.cleanup_now()?;

    Ok(DumpOutcome {
        base_address: address,
        bytes,
        blocks_read,
    })
}

/// Read `[address, address + length)` from the ECU into a flat image.
///
/// Blocks are requested in `block_size` chunks. Cleanup is guaranteed.
pub fn dump_ecu<D, S>(
    device: &mut D,
    session: &mut S,
    address: u32,
    length: usize,
    block_size: usize,
) -> Result<DumpOutcome, FlashError>
where
    D: Device + ?Sized,
    S: KernelSession + ?Sized,
{
    dump_ecu_with_progress(device, session, address, length, block_size, |_, _, _| {})
}

/// Write `image` to the ECU according to `plan`, then verify, reporting per-block progress.
///
/// All length validation happens **before** the kernel is started, so a
/// mismatched image never touches the ECU.
pub fn flash_ecu_with_progress<D, S, F>(
    _device: &mut D,
    session: &mut S,
    plan: &FlashPlan,
    image: &[u8],
    block_size: usize,
    mut on_progress: F,
) -> Result<FlashOutcome, FlashError>
where
    D: Device + ?Sized,
    S: KernelSession + ?Sized,
    F: FnMut(FlashTransferStage, usize, usize, usize),
{
    if plan.ranges.is_empty() {
        return Err(FlashError::EmptyPlan);
    }
    if plan.total_bytes() == 0 {
        // A plan that writes nothing is a caller bug, not a no-op: it would
        // still start and stop a kernel on a live ECU for no reason.
        return Err(FlashError::EmptyPlan);
    }
    if block_size == 0 {
        return Err(FlashError::EmptyPlan);
    }

    // Overlap check. Runs before any device call, because a plan with
    // overlapping ranges would produce an order-dependent image while every
    // individual write still reported success.
    let regions: Vec<crate::region_overlap::FlashRegion> = plan
        .ranges
        .iter()
        .map(|(address, length)| crate::region_overlap::FlashRegion {
            address: *address,
            length: *length,
        })
        .collect();
    if crate::region_overlap::check_no_overlap(&regions).is_err() {
        return Err(FlashError::OverlappingRanges);
    }

    for &(range_addr, range_len) in &plan.ranges {
        validate_u32_range(range_addr, range_len)?;
    }

    let needed = plan.checked_total_bytes()?;
    if image.len() < needed {
        return Err(FlashError::ImageTooShort {
            needed,
            got: image.len(),
        });
    }
    if image.len() > needed {
        return Err(FlashError::ImageTooLong {
            expected: needed,
            got: image.len(),
        });
    }

    let mut guard = CleanupGuard::new(session);
    guard.session.start(KernelOperation::Write)?;

    let mut blocks_written = 0usize;
    let mut cursor = 0usize;
    for &(range_addr, range_len) in &plan.ranges {
        on_progress(FlashTransferStage::Erasing, cursor, needed, blocks_written);
        guard.session.erase_range(range_addr, range_len)?;
        let mut offset = 0usize;
        while offset < range_len {
            let want = (range_len - offset).min(block_size);
            let chunk = &image[cursor + offset..cursor + offset + want];
            let write_addr = range_addr
                .checked_add(u32::try_from(offset).map_err(|_| FlashError::AddressOverflow)?)
                .ok_or(FlashError::AddressOverflow)?;
            guard.session.write_range(write_addr, chunk)?;
            blocks_written += 1;
            offset += want;
            on_progress(
                FlashTransferStage::Writing,
                cursor + offset,
                needed,
                blocks_written,
            );
        }
        cursor += range_len;
    }

    // Verify each range against what the ECU now holds.
    let mut total_mismatches = 0usize;
    let mut cursor = 0usize;
    for &(range_addr, range_len) in &plan.ranges {
        on_progress(FlashTransferStage::Verifying, cursor, needed, blocks_written);
        let expected = &image[cursor..cursor + range_len];
        let mismatches = guard.session.compare_image(range_addr, expected)?;
        total_mismatches += mismatches.len();
        cursor += range_len;
        on_progress(FlashTransferStage::Verifying, cursor, needed, blocks_written);
    }

    guard.cleanup_now()?;

    if total_mismatches > 0 {
        return Err(FlashError::VerificationFailed {
            mismatch_count: total_mismatches,
        });
    }

    Ok(FlashOutcome {
        verified: true,
        bytes_written: needed,
        blocks_written,
    })
}

/// Write `image` to the ECU according to `plan`, then verify.
///
/// All length validation happens **before** the kernel is started, so a
/// mismatched image never touches the ECU.
pub fn flash_ecu<D, S>(
    device: &mut D,
    session: &mut S,
    plan: &FlashPlan,
    image: &[u8],
    block_size: usize,
) -> Result<FlashOutcome, FlashError>
where
    D: Device + ?Sized,
    S: KernelSession + ?Sized,
{
    flash_ecu_with_progress(device, session, plan, image, block_size, |_, _, _, _| {})
}
