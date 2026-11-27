//! Honda bike Keihin K-Line probe and bootloader entry.
//!
//! ## Where these frames come from
//!
//! Extracted from `module76.dll` (Honda bike kline) by reading the `push`
//! immediates that build each frame — the same method that decoded the Denso
//! boot sequence. Two frames are byte-verified against the binary:
//!
//! ```text
//! probe:            68 82 82 00 09   →  [0x82, 0x82, 0x00, 0x09, ...]
//! bootloader entry: 68 50 47 4d 94   →  "PGM" + 0x94
//!                   68 7d 08 01 02   →  [0x7D, 0x08, 0x01, 0x02, ...]
//! ```
//!
//! ## Why the address byte order matters
//!
//! The address in a probe frame is **not** big-endian and **not** little-endian.
//! It is `[bits 16..23, bits 0..7, bits 8..15]` — the low byte in the middle.
//! Sending a normal big-endian address produces a frame the ECU ignores, with no
//! error, which is the hardest kind of failure to diagnose.
//!
//! ## Coverage
//!
//! Keihin M32R at 56/128/192/256/384/512 KB and R8C at 96 KB, per the module's
//! own `FileDescription` and the size table in the binary.

/// MCU flash size this protocol supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McuSize {
    Kb56,
    Kb128,
    Kb192,
    Kb256,
    Kb384,
    Kb512,
    /// R8C variant, 96 KB.
    R8c96,
    /// Size not recognised.
    Unknown,
}

impl McuSize {
    /// Size in bytes; `0` for [`McuSize::Unknown`].
    pub fn bytes(self) -> usize {
        match self {
            McuSize::Kb56 => 56 * 1024,
            McuSize::Kb128 => 128 * 1024,
            McuSize::Kb192 => 192 * 1024,
            McuSize::Kb256 => 256 * 1024,
            McuSize::Kb384 => 384 * 1024,
            McuSize::Kb512 => 512 * 1024,
            McuSize::R8c96 => 96 * 1024,
            McuSize::Unknown => 0,
        }
    }
}

/// Two's-complement checksum over `data`.
///
/// `checksum = (!sum + 1) & 0xFF`, the same 8-bit form the Honda K-Line framing
/// already uses. Exposed separately so a caller can verify a received frame.
pub fn probe_checksum(data: &[u8]) -> u8 {
    let sum: u8 = data.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    (!sum).wrapping_add(1)
}

/// Encode an address in the order this protocol uses.
///
/// `[bits 16..23, bits 0..7, bits 8..15]` — low byte in the middle. Verified
/// against the pushed immediate `00 fc df 04` for address `0x00DFFC`.
fn address_bytes(address: u32) -> [u8; 3] {
    [
        ((address >> 16) & 0xFF) as u8,
        (address & 0xFF) as u8,
        ((address >> 8) & 0xFF) as u8,
    ]
}

/// Build the 9-byte probe frame.
///
/// `[0x82, 0x82, 0x00, 0x09, addr16..23, addr0..7, addr8..15, len, checksum]`
pub fn build_probe_frame(address: u32, length: u8) -> Vec<u8> {
    let mut f = Vec::with_capacity(9);
    f.extend_from_slice(&[0x82, 0x82, 0x00, 0x09]);
    f.extend_from_slice(&address_bytes(address));
    f.push(length);
    let cs = probe_checksum(&f);
    f.push(cs);
    f
}

/// Build a read frame.
///
/// The probe and the read share the same layout; the distinction in the vendor
/// binary is the length field, not the shape. Kept as a separate name so call
/// sites read correctly.
pub fn build_read_frame(address: u32, length: u8) -> Vec<u8> {
    build_probe_frame(address, length)
}

/// Build the 8-byte bootloader entry frame.
///
/// `[0x7D, 0x08, 0x01, 0x02, 'P', 'G', 'M', 0x94]`
///
/// The `0x94` terminator follows the marker in the pushed immediate
/// `68 50 47 4d 94`, so it is part of the frame rather than a checksum of it.
pub fn build_bootloader_entry() -> Vec<u8> {
    let mut f = Vec::with_capacity(8);
    f.extend_from_slice(&[0x7D, 0x08, 0x01, 0x02]);
    f.extend_from_slice(b"PGM");
    f.push(0x94);
    f
}
