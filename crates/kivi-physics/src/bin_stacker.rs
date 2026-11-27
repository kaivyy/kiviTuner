//! Ported from `ecu_engine_rust::physics::bin_stacker`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Bin Stacker & Multi-Bank Mirroring Engine.
pub struct BinStacker;

impl BinStacker {
    /// Mirror a smaller binary file into a larger EPROM target chip.
    ///
    /// For example, placing a 16 KB ($6E) bin into a 32 KB (AT29C256) or 64 KB (27C512) chip.
    /// If the high address pin (A14/A15) is floating, tied HIGH, or tied LOW by the vehicle ECU socket,
    /// mirroring guarantees the engine will find identical valid code at any bank location.
    pub fn mirror_binary_to_size(source_bin: &[u8], target_chip_bytes: usize) -> Result<Vec<u8>, &'static str> {
        let src_len = source_bin.len();
        if src_len == 0 {
            return Err("Source binary cannot be empty");
        }
        if target_chip_bytes < src_len {
            return Err("Target chip size must be greater than or equal to source binary size");
        }
        if target_chip_bytes % src_len != 0 {
            return Err("Target chip size must be an exact power-of-two multiple of source binary size");
        }

        let repetitions = target_chip_bytes / src_len;
        let mut output = Vec::with_capacity(target_chip_bytes);
        for _ in 0..repetitions {
            output.extend_from_slice(source_bin);
        }
        Ok(output)
    }

    /// Compile multiple distinct tune binaries into a multi-bank switchable chip layout.
    ///
    /// Parameters:
    /// - `bins`: Ordered slice of individual binary byte slices (e.g. [ValetTune, DailyTune, RaceTune])
    /// - `switch_block_bytes`: Size allocated to each switch position (e.g. 16384 bytes / 16 KB)
    /// - `total_chip_bytes`: Total physical capacity of the EEPROM (e.g. 65536 bytes / 64 KB)
    pub fn stack_multi_bank(
        bins: &[&[u8]],
        switch_block_bytes: usize,
        total_chip_bytes: usize,
    ) -> Result<Vec<u8>, &'static str> {
        if switch_block_bytes == 0 || total_chip_bytes == 0 {
            return Err("Block size and chip size must be non-zero");
        }
        let total_slots = total_chip_bytes / switch_block_bytes;
        if bins.len() > total_slots {
            return Err("Number of input bins exceeds available hardware switch slots");
        }

        let mut output = vec![0xFFu8; total_chip_bytes]; // Pre-fill with blank EPROM state (0xFF)

        for (slot_idx, &bin_data) in bins.iter().enumerate() {
            let slot_offset = slot_idx * switch_block_bytes;
            if bin_data.len() > switch_block_bytes {
                return Err("Input binary exceeds switch block allocation size");
            }

            // If binary is smaller than the block, mirror it within the block
            let repetitions = switch_block_bytes / bin_data.len();
            for rep in 0..repetitions {
                let dest_start = slot_offset + (rep * bin_data.len());
                let dest_end = dest_start + bin_data.len();
                output[dest_start..dest_end].copy_from_slice(bin_data);
            }
        }

        Ok(output)
    }

    /// Split a stacked multi-bank ROM back into its individual component binaries.
    pub fn split_stacked_binary(
        stacked_bin: &[u8],
        single_bin_bytes: usize,
    ) -> Result<Vec<Vec<u8>>, &'static str> {
        if single_bin_bytes == 0 || stacked_bin.is_empty() {
            return Err("Invalid bin size or empty stacked binary");
        }
        if stacked_bin.len() % single_bin_bytes != 0 {
            return Err("Stacked binary length is not an exact multiple of single bin size");
        }

        let count = stacked_bin.len() / single_bin_bytes;
        let mut individual_bins = Vec::with_capacity(count);

        for i in 0..count {
            let start = i * single_bin_bytes;
            let end = start + single_bin_bytes;
            individual_bins.push(stacked_bin[start..end].to_vec());
        }

        Ok(individual_bins)
    }
}
