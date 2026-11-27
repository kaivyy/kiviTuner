//! Pre-flash safety gate.
//!
//! ## Where these rules come from
//!
//! Safety checks enforced before flashing a ROM image:
//!
//! | Message in the tool | Rule it implies |
//! |---|---|
//! | `Error: Checksum error, maybe selected ROM size is incorrect` | image size must equal the target ROM size |
//! | `Preliminary reading ROM` | read the ECU before writing it |
//! | `Error: Unsupported ECU` | refuse an ECU that is not known |
//! | `Before write tuning, you need write this file use tab Stock…` | stock must be written first |
//! | `Warning: you try to disable checksum correction…` | disabling checksum needs consent |
//!
//! None of these need hardware, so none of them are left as advice.
//!
//! ## Ordering
//!
//! Checks run cheapest-and-most-obvious first. A wrong-size image is usually a
//! typo, so reporting "no backup" before "wrong size" would send the user
//! chasing the wrong problem.
//!
//! ## What this does NOT do
//!
//! It does not read the ECU, does not verify a checksum, and does not know
//! whether a stock image is *correct* — only whether the caller says one exists.
//! Those are caller responsibilities, stated here so the gate is not mistaken for
//! a complete safety system.

/// What the caller knows about the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashTarget {
    /// Size of the target ROM in bytes.
    pub rom_size: usize,
    /// Physical base address the image is expressed against.
    pub base_address: u32,
    /// Whether this ECU appears in a known catalogue.
    pub is_known_ecu: bool,
    /// Whether a stock image has been captured at some point.
    pub has_stock_backup: bool,
    /// Whether a stock image has actually been written to this ECU.
    pub stock_written_first: bool,
    /// Whether checksum correction will run after the write.
    pub checksum_correction_enabled: bool,
}

/// How strict the gate is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashSafetyPolicy {
    pub require_known_ecu: bool,
    pub require_stock_backup: bool,
    pub require_stock_written_first: bool,
    /// When `false`, a write with checksum correction disabled is refused.
    pub allow_checksum_disabled: bool,
}

impl FlashSafetyPolicy {
    /// Every rule on. The default for anything that touches real hardware.
    pub fn strict() -> Self {
        Self {
            require_known_ecu: true,
            require_stock_backup: true,
            require_stock_written_first: true,
            allow_checksum_disabled: false,
        }
    }

    /// Only the size rule. For a bench test where the other inputs are unknown.
    ///
    /// Size is deliberately **not** optional here: a wrong-size write is a brick
    /// risk regardless of how permissive the caller wants to be.
    pub fn permissive() -> Self {
        Self {
            require_known_ecu: false,
            require_stock_backup: false,
            require_stock_written_first: false,
            allow_checksum_disabled: true,
        }
    }
}

impl Default for FlashSafetyPolicy {
    fn default() -> Self {
        Self::strict()
    }
}

/// Why a flash was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlashSafetyError {
    /// The image is not the size of the target ROM.
    SizeMismatch { rom_size: usize, image_size: usize },
    /// The ECU is not in a known catalogue and the policy requires it.
    UnknownEcu,
    /// No stock image has been captured.
    NoStockBackup,
    /// A stock image exists but was never written to this ECU.
    StockNotWrittenFirst,
    /// Checksum correction is off and the policy does not consent.
    ChecksumDisabledWithoutConsent,
}

impl std::fmt::Display for FlashSafetyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlashSafetyError::SizeMismatch {
                rom_size,
                image_size,
            } => write!(
                f,
                "image is {image_size} bytes but the target ROM is {rom_size} bytes; \
                 a wrong-size write bricks the ECU"
            ),
            FlashSafetyError::UnknownEcu => f.write_str(
                "this ECU is not in the known catalogue; flash it at your own risk \
                 or add it explicitly",
            ),
            FlashSafetyError::NoStockBackup => f.write_str(
                "no stock image has been captured; read the ECU before writing so a \
                 failed flash can be undone",
            ),
            FlashSafetyError::StockNotWrittenFirst => f.write_str(
                "a stock image exists but was never written to this ECU; write stock \
                 first so the tuning write has a recovery path",
            ),
            FlashSafetyError::ChecksumDisabledWithoutConsent => f.write_str(
                "checksum correction is disabled; this leaves an invalid checksum in \
                 the ROM and needs explicit consent",
            ),
        }
    }
}

impl std::error::Error for FlashSafetyError {}

/// Run every applicable check.
///
/// Returns `Ok(())` only when the write may proceed.
pub fn check_flash_safety(
    target: &FlashTarget,
    image: &[u8],
    policy: &FlashSafetyPolicy,
) -> Result<(), FlashSafetyError> {
    // 1. Size — always checked, never optional.
    if image.len() != target.rom_size {
        return Err(FlashSafetyError::SizeMismatch {
            rom_size: target.rom_size,
            image_size: image.len(),
        });
    }

    // 2. Known ECU.
    if policy.require_known_ecu && !target.is_known_ecu {
        return Err(FlashSafetyError::UnknownEcu);
    }

    // 3. Stock backup exists.
    if policy.require_stock_backup && !target.has_stock_backup {
        return Err(FlashSafetyError::NoStockBackup);
    }

    // 4. Stock was written before this tuning write.
    if policy.require_stock_written_first && !target.stock_written_first {
        return Err(FlashSafetyError::StockNotWrittenFirst);
    }

    // 5. Checksum correction consent.
    if !target.checksum_correction_enabled && !policy.allow_checksum_disabled {
        return Err(FlashSafetyError::ChecksumDisabledWithoutConsent);
    }

    Ok(())
}
