//! CPU virtual to raw file-offset address translation.
//!
//! Ported from `ecu_engine_rust::math::address_translation`
//! (`MemoryAddressTranslator`, `MemorySegmentRule`, `cpu_to_raw`, `raw_to_cpu`).
//!
//! ## What this maps
//!
//! A list of ordered rules, each describing a CPU address window and the raw
//! file offset its first byte lands at. The first rule whose window contains the
//! address wins; translation inside a window is a linear delta.
//!
//! ## Documented deviation
//!
//! The source returns a bare `u32` and applies a silent **identity** mapping when
//! no rule matches. Silent identity is indistinguishable from a real mapping, so
//! this module returns [`Translation`], which carries the value *and* an
//! `is_identity_fallback` flag. Callers that want the old behaviour read
//! `.value` and ignore the flag; callers that care can refuse to treat an
//! unverified alignment as verified.
//!
//! Arithmetic is checked: a rule whose delta would overflow `u32` returns an
//! error instead of wrapping.
//!
//! ## Not ported
//!
//! `BusDescrambler` — hardware bus-line descrambling. Security-sensitive;
//! remains blocked and is deliberately absent from this module.
//!
//! This module performs no I/O and is deterministic.

/// One CPU-to-raw address mapping rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySegmentRule {
    /// Human-readable segment name.
    pub name: String,
    /// First CPU address covered (inclusive).
    pub cpu_start_address: u32,
    /// Last CPU address covered (inclusive).
    pub cpu_end_address: u32,
    /// Raw file offset corresponding to `cpu_start_address`.
    pub raw_file_offset: u32,
}

/// Result of a translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Translation {
    /// The translated address or offset.
    pub value: u32,
    /// True when no rule matched and the input was returned unchanged.
    ///
    /// A caller must not treat a fallback result as a verified mapping.
    pub is_identity_fallback: bool,
}

/// Ordered set of CPU-to-raw mapping rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryAddressTranslator {
    /// Rules, evaluated in order; the first match wins.
    pub segments: Vec<MemorySegmentRule>,
}

impl Default for MemoryAddressTranslator {
    /// The source's default architecture rules (Infineon TriCore TC17xx/TC2xx
    /// and Renesas RH850 data flash).
    fn default() -> Self {
        Self {
            segments: vec![
                MemorySegmentRule {
                    name: "TriCore Internal Flash (Cached)".to_string(),
                    cpu_start_address: 0x8000_0000,
                    cpu_end_address: 0x803F_FFFF,
                    raw_file_offset: 0x0000_0000,
                },
                MemorySegmentRule {
                    name: "TriCore Internal Flash (Non-Cached)".to_string(),
                    cpu_start_address: 0xA000_0000,
                    cpu_end_address: 0xA03F_FFFF,
                    raw_file_offset: 0x0000_0000,
                },
                MemorySegmentRule {
                    name: "Renesas RH850 DataFlash".to_string(),
                    cpu_start_address: 0xFF20_0000,
                    cpu_end_address: 0xFF20_7FFF,
                    raw_file_offset: 0x0010_0000,
                },
            ],
        }
    }
}

/// Translates a CPU address to a raw file offset.
///
/// Returns [`Translation::is_identity_fallback`] when no rule matched.
pub fn cpu_to_raw(
    translator: &MemoryAddressTranslator,
    cpu_addr: u32,
) -> Result<Translation, String> {
    for seg in &translator.segments {
        if cpu_addr >= seg.cpu_start_address && cpu_addr <= seg.cpu_end_address {
            let delta = cpu_addr - seg.cpu_start_address;
            let value = seg.raw_file_offset.checked_add(delta).ok_or_else(|| {
                format!(
                    "Address translation overflow: raw_file_offset {:#010X} + delta {:#X} exceeds u32",
                    seg.raw_file_offset, delta
                )
            })?;
            return Ok(Translation {
                value,
                is_identity_fallback: false,
            });
        }
    }
    Ok(Translation {
        value: cpu_addr,
        is_identity_fallback: true,
    })
}

/// Translates a raw file offset back to a CPU address.
///
/// Returns [`Translation::is_identity_fallback`] when no rule matched.
pub fn raw_to_cpu(
    translator: &MemoryAddressTranslator,
    raw_offset: u32,
) -> Result<Translation, String> {
    for seg in &translator.segments {
        let seg_size = seg.cpu_end_address.checked_sub(seg.cpu_start_address).ok_or_else(|| {
            format!(
                "Invalid address translation segment: cpu_end_address {:#010X} < cpu_start_address {:#010X}",
                seg.cpu_end_address, seg.cpu_start_address
            )
        })?;
        let window_end = seg.raw_file_offset.checked_add(seg_size).ok_or_else(|| {
            format!(
                "Address translation overflow: raw_file_offset {:#010X} + size {:#X} exceeds u32",
                seg.raw_file_offset, seg_size
            )
        })?;
        if raw_offset >= seg.raw_file_offset && raw_offset <= window_end {
            let delta = raw_offset - seg.raw_file_offset;
            let value = seg.cpu_start_address.checked_add(delta).ok_or_else(|| {
                format!(
                    "Address translation overflow: cpu_start_address {:#010X} + delta {:#X} exceeds u32",
                    seg.cpu_start_address, delta
                )
            })?;
            return Ok(Translation {
                value,
                is_identity_fallback: false,
            });
        }
    }
    Ok(Translation {
        value: raw_offset,
        is_identity_fallback: true,
    })
}

// ---------------------------------------------------------------------------
// Bus descrambling (ported from `ecu_engine_rust::math::address_translation`
// `BusDescrambler`). Authorized by the project owner; security-sensitive.
// ---------------------------------------------------------------------------

/// Hardware bus-line descrambling helpers.
///
/// Used to reconstruct a linear image from a dump taken off physically permuted
/// address/data lines (e.g. VAG Simos, Bosch ME9). **Security-sensitive:** these
/// reverse deliberate anti-readout scrambling on hardware the owner controls.
pub struct BusDescrambler;

impl BusDescrambler {
    /// XORs every byte with a mask. Applying it twice restores the input.
    pub fn xor_scramble(data: &mut [u8], xor_mask: u8) {
        for b in data.iter_mut() {
            *b ^= xor_mask;
        }
    }

    /// Reverses the bit order of every byte (`D0 <-> D7`).
    pub fn reverse_data_bits(data: &mut [u8]) {
        for b in data.iter_mut() {
            *b = b.reverse_bits();
        }
    }

    /// Rebuilds a linear image from an address-line-permuted dump.
    ///
    /// `address_bit_mapping` maps each logical bit index to its physical source
    /// bit. Bytes whose computed address falls outside the buffer are dropped.
    pub fn descramble_address_permutation(
        raw_rom: &[u8],
        address_bit_mapping: [u8; 16],
    ) -> Vec<u8> {
        let size = raw_rom.len();
        let mut descrambled = vec![0u8; size];

        for (old_addr, &val) in raw_rom.iter().enumerate() {
            let old_u32 = old_addr as u32;
            let mut new_addr = old_u32 & !0xFFFFu32;

            for (target_bit, &source_bit) in address_bit_mapping.iter().enumerate() {
                if source_bit < 32 && (old_u32 & (1u32 << source_bit)) != 0 {
                    new_addr |= 1u32 << target_bit;
                }
            }

            if (new_addr as usize) < size {
                descrambled[new_addr as usize] = val;
            }
        }
        descrambled
    }
}

// ---------------------------------------------------------------------------
// ME7 (Bosch Motronic) preset
// ---------------------------------------------------------------------------

/// First address of the ME7 flash window.
pub const ME7_FLASH_BASE: u32 = 0x0080_0000;

/// Last address of the ME7 flash window on a 1 MiB part.
pub const ME7_FLASH_END: u32 = 0x008F_FFFF;

/// First address of the ME7 internal RAM window.
pub const ME7_RAM_START: u32 = 0x0038_0000;

/// Last address of the ME7 internal RAM window.
pub const ME7_RAM_END: u32 = 0x003F_FFFF;

/// Where an ME7 ECU address points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Me7Translation {
    /// The address is flash, and its contents live at this file offset.
    FileOffset(u32),
    /// The address is internal RAM — a runtime variable with no file image.
    InternalRam,
    /// The address belongs to neither window.
    Unmapped,
}

/// Translate an ME7 ECU address.
///
/// ## Why a DAMOS address needs this
///
/// A DAMOS file addresses the ECU, not the file: `$810C4A` is flash, `$38487C`
/// is internal RAM. Comparing either against a BIN offset without translating
/// compares different things.
///
/// ## Why RAM is called out separately
///
/// Roughly four fifths of the maps in a typical ME7 DAMOS live in the RAM
/// window. Those are runtime variables; a ROM dump cannot contain them, so a
/// caller must not treat them as "not found in the file" — they were never
/// going to be there.
pub fn translate_me7(address: u32) -> Me7Translation {
    if (ME7_FLASH_BASE..=ME7_FLASH_END).contains(&address) {
        return Me7Translation::FileOffset(address - ME7_FLASH_BASE);
    }
    if (ME7_RAM_START..=ME7_RAM_END).contains(&address) {
        return Me7Translation::InternalRam;
    }
    Me7Translation::Unmapped
}

/// Whether `address` translates to a file offset inside a ROM of `rom_len` bytes.
///
/// Returns `false` for RAM addresses: they never have a file image, regardless
/// of ROM size.
pub fn me7_offset_within(address: u32, rom_len: usize) -> bool {
    match translate_me7(address) {
        Me7Translation::FileOffset(off) => (off as usize) < rom_len,
        Me7Translation::InternalRam | Me7Translation::Unmapped => false,
    }
}
