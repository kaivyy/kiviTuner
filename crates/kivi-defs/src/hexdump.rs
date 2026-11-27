//! Ported from `ecu_engine_rust::ui::hexdump`.
//!
//! Pure logic, no I/O. Verified by unit tests where applicable; not validated
//! against physical hardware.

/// Single Byte Cell Visual State.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteDeltaStatus {
    Unchanged,
    Increased, // Highlight Green
    Decreased, // Highlight Blue
    Unmapped,  // Program code or padding
}

/// Formatted Hexdump Row for Virtualized UI Display.
#[derive(Debug, Clone)]
pub struct HexdumpRowView {
    pub offset_address: usize,
    pub address_label: String,
    pub hex_bytes: Vec<(String, ByteDeltaStatus)>,
    pub ascii_chars: Vec<(char, ByteDeltaStatus)>,
}

/// Virtualized Hexdump ViewModel.
pub struct HexdumpViewModel {
    pub bytes_per_row: usize, // Typically 16 bytes per line
    pub line_height_px: f64,
}

impl Default for HexdumpViewModel {
    fn default() -> Self {
        Self {
            bytes_per_row: 16,
            line_height_px: 20.0,
        }
    }
}

impl HexdumpViewModel {
    /// Calculate total virtual line count for scrollbar sizing.
    pub fn total_line_count(&self, total_bytes: usize) -> usize {
        (total_bytes + self.bytes_per_row - 1) / self.bytes_per_row
    }

    /// Calculate the visible row index range [start_row..end_row] given viewport scroll position.
    pub fn calculate_visible_rows(&self, scroll_y_px: f64, viewport_height_px: f64) -> (usize, usize) {
        let start_row = (scroll_y_px / self.line_height_px).floor().max(0.0) as usize;
        let visible_count = (viewport_height_px / self.line_height_px).ceil() as usize + 2; // +2 for buffer padding
        (start_row, start_row + visible_count)
    }

    /// Render visible rows comparing active version against original baseline bytes.
    pub fn generate_visible_rows(
        &self,
        active_rom: &[u8],
        original_rom: &[u8],
        start_row: usize,
        end_row: usize,
    ) -> Vec<HexdumpRowView> {
        let total_lines = self.total_line_count(active_rom.len());
        let clamped_end = end_row.min(total_lines);

        let mut rows = Vec::with_capacity(clamped_end.saturating_sub(start_row));

        for line_idx in start_row..clamped_end {
            let row_offset = line_idx * self.bytes_per_row;
            let end_offset = (row_offset + self.bytes_per_row).min(active_rom.len());

            let mut hex_cells = Vec::with_capacity(self.bytes_per_row);
            let mut ascii_cells = Vec::with_capacity(self.bytes_per_row);

            for offset in row_offset..end_offset {
                let cur_byte = active_rom[offset];
                let orig_byte = original_rom.get(offset).copied().unwrap_or(cur_byte);

                let status = if cur_byte == orig_byte {
                    ByteDeltaStatus::Unchanged
                } else if cur_byte > orig_byte {
                    ByteDeltaStatus::Increased
                } else {
                    ByteDeltaStatus::Decreased
                };

                let hex_str = format!("{:02X}", cur_byte);
                let ascii_char = if cur_byte >= 32 && cur_byte < 127 {
                    cur_byte as char
                } else {
                    '.'
                };

                hex_cells.push((hex_str, status));
                ascii_cells.push((ascii_char, status));
            }

            rows.push(HexdumpRowView {
                offset_address: row_offset,
                address_label: format!("{:06X}", row_offset),
                hex_bytes: hex_cells,
                ascii_chars: ascii_cells,
            });
        }

        rows
    }
}
