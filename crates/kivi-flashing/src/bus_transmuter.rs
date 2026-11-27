//! Ported from `ecu_engine_rust::security::bus_transmuter`.
//!
//! Pure logic, no I/O. Verified by unit tests where applicable; not validated
//! against physical hardware.

/// Arbitrary 8-Bit Data Pin Permutation.
/// Maps each logical bit position [0..7] to its physical chip pin index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPinPermutation {
    pub pin_map: [u8; 8], // e.g. [7, 6, 5, 4, 3, 2, 1, 0] for bit reverse, or custom scramble
}

impl Default for DataPinPermutation {
    fn default() -> Self {
        // Identity mapping (pin 0 -> 0, pin 1 -> 1, ...)
        Self {
            pin_map: [0, 1, 2, 3, 4, 5, 6, 7],
        }
    }
}

impl DataPinPermutation {
    /// Create a standard bit-reverse permutation (D0 <-> D7, D1 <-> D6, etc.).
    pub fn reversed() -> Self {
        Self {
            pin_map: [7, 6, 5, 4, 3, 2, 1, 0],
        }
    }

    /// Generate the exact inverse permutation for reverse transmutation.
    pub fn invert(&self) -> Result<Self, &'static str> {
        let mut inv = [0u8; 8];
        let mut seen = [false; 8];

        for (src_bit, &target_pin) in self.pin_map.iter().enumerate() {
            if target_pin >= 8 {
                return Err("Data pin index out of range [0..7]");
            }
            if seen[target_pin as usize] {
                return Err("Duplicate data pin mapped in permutation");
            }
            seen[target_pin as usize] = true;
            inv[target_pin as usize] = src_bit as u8;
        }

        Ok(Self { pin_map: inv })
    }

    /// Permute the bits of a single 8-bit byte.
    #[inline]
    pub fn permute_byte(&self, byte: u8) -> u8 {
        let mut out = 0u8;
        for (target_pos, &src_pin) in self.pin_map.iter().enumerate() {
            if src_pin < 8 && (byte & (1 << src_pin)) != 0 {
                out |= 1 << target_pos;
            }
        }
        out
    }
}

/// Address Bus Line Permutation.
/// Maps each logical address line A[i] to its physical chip address line A[j].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressBusPermutation {
    pub bit_count: usize, // e.g. 15 for 32KB (27C256), 16 for 64KB (27C512), 19 for 512KB (29F040)
    pub pin_map: Vec<u8>,
}

impl AddressBusPermutation {
    /// Create an address permutation from a slice of physical pin mappings.
    pub fn new(pin_map: &[u8]) -> Result<Self, &'static str> {
        let bit_count = pin_map.len();
        if bit_count > 32 || bit_count < 2 {
            return Err("Address bus width must be between 2 and 32 bits");
        }

        // Validate uniqueness
        let mut seen = vec![false; bit_count];
        for &pin in pin_map {
            let p = pin as usize;
            if p >= bit_count {
                return Err("Address pin index out of range");
            }
            if seen[p] {
                return Err("Duplicate address pin in permutation");
            }
            seen[p] = true;
        }

        Ok(Self {
            bit_count,
            pin_map: pin_map.to_vec(),
        })
    }

    /// Generate inverse address permutation.
    pub fn invert(&self) -> Self {
        let mut inv = vec![0u8; self.bit_count];
        for (src_bit, &target_pin) in self.pin_map.iter().enumerate() {
            inv[target_pin as usize] = src_bit as u8;
        }
        Self {
            bit_count: self.bit_count,
            pin_map: inv,
        }
    }

    /// Permute a single address offset.
    #[inline]
    pub fn permute_address(&self, addr: u32) -> u32 {
        let mut out = 0u32;
        for (target_pos, &src_pin) in self.pin_map.iter().enumerate() {
            if (addr & (1 << src_pin)) != 0 {
                out |= 1 << target_pos;
            }
        }
        out
    }
}

/// Cryptographic Data Masking Rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataMaskCipher {
    None,
    StaticXor(u8),
    /// Rolling address-dependent XOR mask (LFSR / address hash encryption)
    AddressDependentXor {
        seed: u8,
        multiplier: u8,
    },
}

impl DataMaskCipher {
    /// Calculate masking byte for a specific logical address.
    #[inline]
    pub fn mask_for_address(&self, address: u32) -> u8 {
        match *self {
            DataMaskCipher::None => 0x00,
            DataMaskCipher::StaticXor(mask) => mask,
            DataMaskCipher::AddressDependentXor { seed, multiplier } => {
                let low_addr = (address & 0xFF) as u8;
                seed.wrapping_add(low_addr.wrapping_mul(multiplier))
            }
        }
    }
}

/// High-Level Hardware Bus Transmuter (Unscrambler & Scrambler).
pub struct SiliconBusTransmuter {
    pub address_perm: Option<AddressBusPermutation>,
    pub data_perm: DataPinPermutation,
    pub cipher: DataMaskCipher,
}

impl SiliconBusTransmuter {
    pub fn new(
        address_perm: Option<AddressBusPermutation>,
        data_perm: DataPinPermutation,
        cipher: DataMaskCipher,
    ) -> Self {
        Self {
            address_perm,
            data_perm,
            cipher,
        }
    }

    /// Forward Scrambler: Protect clean calibration binary before burning to security socket/EPROM.
    /// Order: Permute data bits -> XOR mask with logical address -> Place at permuted address.
    pub fn scramble(&self, clean_rom: &[u8]) -> Vec<u8> {
        let size = clean_rom.len();
        let mut scrambled = vec![0u8; size];

        for (logical_addr, &raw_byte) in clean_rom.iter().enumerate() {
            let log_u32 = logical_addr as u32;

            // 1. Permute data bits
            let permuted = self.data_perm.permute_byte(raw_byte);

            // 2. XOR mask keyed by logical address
            let mask = self.cipher.mask_for_address(log_u32);
            let ciphered_byte = permuted ^ mask;

            // 3. Compute destination physical address (preserve bank/high bits above perm.bit_count)
            let phys_addr = match &self.address_perm {
                Some(perm) => {
                    let high_mask = if perm.bit_count >= 32 {
                        0
                    } else {
                        !((1u32 << perm.bit_count) - 1)
                    };
                    ((log_u32 & high_mask) | perm.permute_address(log_u32)) as usize
                }
                None => logical_addr,
            };

            if phys_addr < size {
                scrambled[phys_addr] = ciphered_byte;
            }
        }

        scrambled
    }

    /// Reverse Descrambler (Unscrambler): Recover 100% clean, original binary from scrambled chip dump.
    /// Order: Read physical address -> Compute logical address -> XOR mask -> Inverse permute data bits.
    pub fn descramble(&self, scrambled_rom: &[u8]) -> Result<Vec<u8>, &'static str> {
        let size = scrambled_rom.len();
        let mut clean = vec![0u8; size];

        let inv_data_perm = self.data_perm.invert()?;
        let inv_addr_perm = self.address_perm.as_ref().map(|p| p.invert());

        for (phys_addr, &ciphered_byte) in scrambled_rom.iter().enumerate() {
            let phys_u32 = phys_addr as u32;

            // 1. Recover original logical address (preserve bank/high bits above inv_p.bit_count)
            let log_addr = match &inv_addr_perm {
                Some(inv_p) => {
                    let high_mask = if inv_p.bit_count >= 32 {
                        0
                    } else {
                        !((1u32 << inv_p.bit_count) - 1)
                    };
                    ((phys_u32 & high_mask) | inv_p.permute_address(phys_u32)) as usize
                }
                None => phys_addr,
            };

            // 2. Remove XOR mask keyed by logical address
            let mask = self.cipher.mask_for_address(log_addr as u32);
            let unmasked_byte = ciphered_byte ^ mask;

            // 3. Reverse data pin permutation
            let original_byte = inv_data_perm.permute_byte(unmasked_byte);

            if log_addr < size {
                clean[log_addr] = original_byte;
            }
        }

        Ok(clean)
    }
}
