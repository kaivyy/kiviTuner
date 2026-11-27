//! The editor must honour XDF row/column stride when reading and writing cells.
//!
//! ## The gap this pins
//!
//! The scanner's parser was fixed to honour `mmedmajorstridebits` /
//! `mmedminorstridebits`. The **editor** has its own parser
//! (`kivi_defs::xdf`) and its own reader (`extract_map_matrix_inner`), and both
//! ignored stride entirely — every padded table was read as if packed.
//!
//! Measured: 1 459 XDF files in the database declare a non-zero stride, and
//! 203 tables in the 647-pair BIN+XDF catalogue are padded (stride differs from
//! `cols * element_size`). Reading those packed returns cells from the wrong row.
//!
//! ## Fixture design
//!
//! A 2x2 table of 16-bit cells with a **minor** stride of 32 bits: each cell
//! occupies 4 bytes and only the first 2 hold the value. The other 2 bytes are
//! sentinels (`0xDEAD`) that must never be read as a cell. This models the real
//! interleaved layout found in the Suzuki Hayabusa definition, where a
//! resume/cut pair shares each 4-byte cell.

use kivi_core::types::{DType, Endian};
use kivi_defs::xdf::parse_xdf_full;
use kivituner::commands::{extract_map_matrix_inner, load_bin_data_inner, AppState};

/// An XDF whose only table has the given z-axis strides, in bits.
fn xdf_with_strides(major_bits: i32, minor_bits: i32) -> String {
    format!(
        r#"<XDFFORMAT version="1.60">
  <XDFHEADER>
    <deftitle>editor stride fixture</deftitle>
    <DEFAULTS datasizeinbits="16" signdata="0" outputtype="1" signedoutput="0" />
    <BASEOFFSET offset="0" subtract="0" />
    <CATEGORY index="0x0" name="Root" />
  </XDFHEADER>
  <XDFTABLE uniqueid="0x1" flags="0x0">
    <title>strided</title>
    <XDFAXIS id="x" uniqueid="0x0">
      <EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="0x0" mmedelementsizebits="16" />
      <indexcount>2</indexcount><units>rpm</units><MATH equation="X" />
    </XDFAXIS>
    <XDFAXIS id="y" uniqueid="0x0">
      <EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="0x0" mmedelementsizebits="16" />
      <indexcount>2</indexcount><units>kpa</units><MATH equation="X" />
    </XDFAXIS>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="0x40" mmedelementsizebits="16"
                    mmedrowcount="2" mmedcolcount="2"
                    mmedmajorstridebits="{major_bits}" mmedminorstridebits="{minor_bits}" />
      <MATH equation="X" />
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#
    )
}

fn put(rom: &mut [u8], at: usize, v: u16) {
    let b = v.to_be_bytes();
    rom[at] = b[0];
    rom[at + 1] = b[1];
}

#[test]
fn a_minor_stride_skips_the_padding_between_cells() {
    // Cell pitch is 32 bits (4 bytes); each cell stores its value in the first
    // two bytes and a sentinel in the last two.
    let defs = parse_xdf_full(&xdf_with_strides(64, 32)).expect("parse");
    let map = &defs.maps[0];

    let mut rom = vec![0u8; 0x100];
    put(&mut rom, 0x40, 1000);
    rom[0x42] = 0xDE;
    rom[0x43] = 0xAD;
    put(&mut rom, 0x44, 2000);
    rom[0x46] = 0xDE;
    rom[0x47] = 0xAD;
    put(&mut rom, 0x48, 3000);
    rom[0x4A] = 0xDE;
    rom[0x4B] = 0xAD;
    put(&mut rom, 0x4C, 4000);

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load");
    let matrix = extract_map_matrix_inner(map, &state).expect("extract");

    assert_eq!(
        matrix,
        vec![vec![1000.0, 2000.0], vec![3000.0, 4000.0]],
        "each cell is 4 bytes apart; the sentinel bytes must never be read"
    );
}

#[test]
fn a_zero_stride_still_reads_packed_tables() {
    let defs = parse_xdf_full(&xdf_with_strides(0, 0)).expect("parse");
    let map = &defs.maps[0];

    let mut rom = vec![0u8; 0x100];
    put(&mut rom, 0x40, 1000);
    put(&mut rom, 0x42, 2000);
    put(&mut rom, 0x44, 3000);
    put(&mut rom, 0x46, 4000);

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load");
    let matrix = extract_map_matrix_inner(map, &state).expect("extract");
    assert_eq!(matrix, vec![vec![1000.0, 2000.0], vec![3000.0, 4000.0]]);
}

#[test]
fn the_parsed_map_carries_the_declared_strides() {
    let defs = parse_xdf_full(&xdf_with_strides(64, 32)).expect("parse");
    let map = &defs.maps[0];
    assert_eq!(map.major_stride_bits, 64, "major stride must be preserved");
    assert_eq!(map.minor_stride_bits, 32, "minor stride must be preserved");
}

#[test]
fn the_parsed_map_reports_the_declared_type() {
    let defs = parse_xdf_full(&xdf_with_strides(0, 0)).expect("parse");
    assert_eq!(defs.maps[0].dtype, DType::U16);
    assert_eq!(defs.maps[0].endian, Endian::Big);
}

#[test]
fn non_square_column_major_tables_read_and_write_without_aliasing_or_oob() {
    use kivi_defs::model::{MapDef, Scaling};
    use kivituner::commands::{patch_map_cells_inner, read_rom_window_inner, CellWriteInput};

    for (rows, cols) in [(4usize, 8usize), (8usize, 4usize)] {
        let base = 0x20usize;
        let len = rows * cols;
        let mut rom = vec![0xAAu8; 0x100];
        for c in 0..cols {
            for r in 0..rows {
                // Packed column-major: column c is stored contiguously of length `rows`.
                rom[base + c * rows + r] = (r * 16 + c) as u8;
            }
        }
        let map = MapDef {
            id: format!("cm-{rows}x{cols}"),
            name: format!("ColMajor {rows}x{cols}"),
            category: "Test".to_string(),
            address: base,
            rows,
            cols,
            dtype: DType::U8,
            endian: Endian::Little,
            column_major: true,
            major_stride_bits: 0,
            minor_stride_bits: 0,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: String::new(),
                decimals: 0,
            },
            x_axis: None,
            y_axis: None,
        };

        let state = AppState::default();
        load_bin_data_inner(&rom, &state).expect("load");
        let matrix = extract_map_matrix_inner(&map, &state).expect("extract");
        assert_eq!(matrix.len(), rows);
        for (r, row) in matrix.iter().enumerate() {
            assert_eq!(row.len(), cols);
            for (c, &val) in row.iter().enumerate() {
                assert_eq!(val, (r * 16 + c) as f64, "aliasing at ({r},{c})");
            }
        }

        patch_map_cells_inner(
            &map,
            &[CellWriteInput {
                row: rows - 1,
                col: cols - 1,
                value: 254.0,
            }],
            Some("Tail write"),
            &state,
        )
        .expect("patch tail cell");

        // Adjacent sentinel bytes after `base + len` must remain untouched 0xAA.
        let after = read_rom_window_inner(base + len, 32, &state).expect("tail read");
        assert!(after.iter().all(|&b| b == 0xAA), "OOB write past column-major table");
        assert_eq!(
            read_rom_window_inner(base + len - 1, 1, &state).expect("last byte"),
            vec![254]
        );
    }
}
