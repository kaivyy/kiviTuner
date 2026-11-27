use crate::model::{DiffKind, DiffRegion};
use kivi_defs::model::{AxisDef, MapDef, Scaling};

/// Converts a `DiffRegion` into a ready-to-use `MapDef`.
pub fn diff_region_to_map_def(region: &DiffRegion) -> MapDef {
    let (rows, cols) = match &region.classification {
        DiffKind::Table2D {
            suggested_rows,
            suggested_cols,
        } => (*suggested_rows, *suggested_cols),
        DiffKind::Axis1D { length } => (1, *length),
        _ => (1, 1),
    };

    let name = match &region.classification {
        DiffKind::Table2D {
            suggested_rows,
            suggested_cols,
        } => format!(
            "Table {}x{} @ 0x{:X}",
            suggested_rows, suggested_cols, region.start_addr
        ),
        DiffKind::Axis1D { length } => format!("Axis {} @ 0x{:X}", length, region.start_addr),
        DiffKind::Scalar => format!("Scalar @ 0x{:X}", region.start_addr),
        DiffKind::ChecksumOrRawBlock => format!("Raw Block @ 0x{:X}", region.start_addr),
    };

    let scaling = Scaling::raw(region.inferred_dtype);

    let x_axis = Some(AxisDef {
        address: 0,
        length: cols,
        dtype: region.inferred_dtype,
        endian: region.inferred_endian,
        scaling: scaling.clone(),
        values: Vec::new(),
    });

    let y_axis = Some(AxisDef {
        address: 0,
        length: rows,
        dtype: region.inferred_dtype,
        endian: region.inferred_endian,
        scaling: scaling.clone(),
        values: Vec::new(),
    });

    MapDef {
        id: format!("map_{}", region.id),
        name,
        category: "Diff Inferred".to_string(),
        address: region.start_addr,
        rows,
        cols,
        dtype: region.inferred_dtype,
        endian: region.inferred_endian,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling,
        x_axis,
        y_axis,
    }
}
