//! Synchronised twin-map coupling and linked delta editing.
//!
//! Ported from `ecu_engine_rust::math::twin_maps` (`CoupledMapPair`,
//! `MapCouplingMode`). Used for multi-cylinder bikes (e.g. Honda CBR250RR
//! cylinder 1 and 2) and V-engine left/right banks, where two maps must move
//! together.
//!
//! ## Preserved source behaviour
//!
//! * `AbsoluteSync` sets the slave cell exactly equal to the master.
//! * `IdenticalDelta` adds the master's delta to the slave, preserving the
//!   slave's baseline offset.
//! * `PercentageDelta` scales the slave by the master's ratio; when the old
//!   master value is within `1e-9` of zero it falls back to the additive branch
//!   rather than dividing by a near-zero.
//! * Coordinates are validated against the pair's declared dimensions.

/// How a slave map follows its master.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapCouplingMode {
    /// Slave cells are set exactly equal to the master's.
    AbsoluteSync,
    /// The master's delta is added to the slave (preserves slave baseline).
    IdenticalDelta,
    /// The master's percentage change is applied to the slave.
    PercentageDelta,
}

/// A coupled master/slave map pair with a synchronised cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct CoupledMapPair {
    /// Identifier of the master map.
    pub master_map_id: String,
    /// Identifier of the slave map.
    pub slave_map_id: String,
    /// Coupling rule.
    pub mode: MapCouplingMode,
    /// Row count shared by both maps.
    pub rows: usize,
    /// Column count shared by both maps.
    pub cols: usize,
    /// Current cursor row.
    pub current_cursor_row: usize,
    /// Current cursor column.
    pub current_cursor_col: usize,
}

impl CoupledMapPair {
    /// Creates a pair with the cursor at the origin.
    pub fn new(
        master_id: &str,
        slave_id: &str,
        rows: usize,
        cols: usize,
        mode: MapCouplingMode,
    ) -> Self {
        Self {
            master_map_id: master_id.to_string(),
            slave_map_id: slave_id.to_string(),
            mode,
            rows,
            cols,
            current_cursor_row: 0,
            current_cursor_col: 0,
        }
    }

    /// Moves the cursor, validating it against the pair's dimensions.
    pub fn move_cursor(&mut self, row: usize, col: usize) -> Result<(), &'static str> {
        if row >= self.rows || col >= self.cols {
            return Err("Cursor coordinates out of bounds");
        }
        self.current_cursor_row = row;
        self.current_cursor_col = col;
        Ok(())
    }

    /// Applies an edit to the master and propagates the linked change.
    pub fn apply_synchronized_edit(
        &self,
        master_matrix: &mut [Vec<f64>],
        slave_matrix: &mut [Vec<f64>],
        row: usize,
        col: usize,
        new_master_value: f64,
    ) -> Result<(), &'static str> {
        if row >= self.rows || col >= self.cols {
            return Err("Edit coordinates out of bounds");
        }

        let old_master_val = master_matrix[row][col];
        let delta = new_master_value - old_master_val;
        master_matrix[row][col] = new_master_value;

        match self.mode {
            MapCouplingMode::AbsoluteSync => {
                slave_matrix[row][col] = new_master_value;
            }
            MapCouplingMode::IdenticalDelta => {
                slave_matrix[row][col] += delta;
            }
            MapCouplingMode::PercentageDelta => {
                if old_master_val.abs() > 1e-9 {
                    let pct_change = (new_master_value - old_master_val) / old_master_val;
                    slave_matrix[row][col] *= 1.0 + pct_change;
                } else {
                    slave_matrix[row][col] += delta;
                }
            }
        }

        Ok(())
    }
}

/// Applies a synchronised edit without constructing a [`CoupledMapPair`].
///
/// Convenience wrapper with the same semantics and validation.
#[allow(clippy::too_many_arguments)]
pub fn apply_synchronized_edit(
    mode: MapCouplingMode,
    rows: usize,
    cols: usize,
    master_matrix: &mut [Vec<f64>],
    slave_matrix: &mut [Vec<f64>],
    row: usize,
    col: usize,
    new_master_value: f64,
) -> Result<(), &'static str> {
    CoupledMapPair {
        master_map_id: String::new(),
        slave_map_id: String::new(),
        mode,
        rows,
        cols,
        current_cursor_row: 0,
        current_cursor_col: 0,
    }
    .apply_synchronized_edit(master_matrix, slave_matrix, row, col, new_master_value)
}
