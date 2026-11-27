//! XDF row stride must be honoured when reading table cells.
//!
//! ## The gap this pins
//!
//! `mmedmajorstridebits` declares the distance in **bits** between the start of
//! one row and the start of the next. When it is `0` the rows are contiguous.
//! When it is non-zero and differs from `cols * element_size_bits`, the table is
//! **padded**: rows are not adjacent, and reading it as a contiguous block
//! silently returns cells from the wrong rows.
//!
//! kiviTuner ignored the attribute entirely, so every padded table was read as
//! if it were packed. Measured across the database: 3 032 XDF files carry the
//! tag and 179 134 stride values are non-zero.
//!
//! ## Fixture design
//!
//! A 2x2 table of 16-bit cells with a row stride of 8 bytes (64 bits) instead of
//! the packed 4 bytes. Between the two rows sit two sentinel bytes `0xDEAD` that
//! must never appear in the result. A packed read would pick them up, so the
//! test fails loudly rather than returning a plausible-looking wrong table.

use kivi_scanner::xdf_parse::parse_xdf_xml;

/// Builds an XDF whose single table has the given z-axis stride, in bits.
fn xdf_with_stride(stride_bits: i32) -> String {
    xdf_with_stride_at(stride_bits, 0x40)
}

/// Same, with an explicit z-axis base address.
fn xdf_with_stride_at(stride_bits: i32, z_address: usize) -> String {
    format!(
        r#"<XDFFORMAT version="1.60">
  <XDFHEADER>
    <deftitle>stride fixture</deftitle>
    <DEFAULTS datasizeinbits="16" signdata="0" outputtype="1" signedoutput="0" />
    <CATEGORY index="0x0" name="Root" />
    <BASEOFFSET offset="0" subtract="0" />
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
      <EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="0x{z_address:02X}" mmedelementsizebits="16"
                    mmedrowcount="2" mmedcolcount="2" mmedmajorstridebits="{stride_bits}" />
      <MATH equation="X" />
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#
    )
}

/// ROM layout: row 0 at 0x40 = [1000, 2000]; row 1 at 0x48 = [3000, 4000];
/// with `0xDE 0xAD` sentinels in the padding between them.
fn padded_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x100];
    let put = |rom: &mut Vec<u8>, at: usize, v: u16| {
        let b = v.to_be_bytes();
        rom[at] = b[0];
        rom[at + 1] = b[1];
    };
    put(&mut rom, 0x40, 1000);
    put(&mut rom, 0x42, 2000);
    rom[0x44] = 0xDE;
    rom[0x45] = 0xAD;
    put(&mut rom, 0x48, 3000);
    put(&mut rom, 0x4A, 4000);
    rom
}

#[test]
fn a_padded_table_skips_the_padding_between_rows() {
    let parsed = parse_xdf_xml(&xdf_with_stride(64)).expect("parse");
    let table = &parsed.tables[0];
    let rom = padded_rom();

    let matrix = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, table)
        .expect("extract failed");

    assert_eq!(
        matrix,
        vec![vec![1000.0, 2000.0], vec![3000.0, 4000.0]],
        "row 1 must start at the declared stride, not immediately after row 0"
    );
    assert!(
        !matrix.iter().flatten().any(|v| *v == 0xDEADu16 as f64),
        "the sentinel padding must never be read as a cell"
    );
}

#[test]
fn a_zero_stride_means_packed_rows() {
    let parsed = parse_xdf_xml(&xdf_with_stride(0)).expect("parse");
    let table = &parsed.tables[0];

    // Packed layout: row 0 at 0x40, row 1 immediately at 0x44.
    let mut rom = vec![0u8; 0x100];
    let put = |rom: &mut Vec<u8>, at: usize, v: u16| {
        let b = v.to_be_bytes();
        rom[at] = b[0];
        rom[at + 1] = b[1];
    };
    put(&mut rom, 0x40, 1000);
    put(&mut rom, 0x42, 2000);
    put(&mut rom, 0x44, 3000);
    put(&mut rom, 0x46, 4000);

    let matrix = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, table)
        .expect("extract failed");
    assert_eq!(matrix, vec![vec![1000.0, 2000.0], vec![3000.0, 4000.0]]);
}

#[test]
fn a_stride_equal_to_the_row_size_behaves_like_packed() {
    // 2 cols * 16 bits = 32 bits is the natural row size.
    let parsed = parse_xdf_xml(&xdf_with_stride(32)).expect("parse");
    let table = &parsed.tables[0];

    let mut rom = vec![0u8; 0x100];
    let put = |rom: &mut Vec<u8>, at: usize, v: u16| {
        let b = v.to_be_bytes();
        rom[at] = b[0];
        rom[at + 1] = b[1];
    };
    put(&mut rom, 0x40, 1000);
    put(&mut rom, 0x42, 2000);
    put(&mut rom, 0x44, 3000);
    put(&mut rom, 0x46, 4000);

    let matrix = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, table)
        .expect("extract failed");
    assert_eq!(matrix, vec![vec![1000.0, 2000.0], vec![3000.0, 4000.0]]);
}

#[test]
fn a_negative_stride_reads_rows_backwards() {
    // -64 bits means each successive row starts 8 bytes earlier.
    let parsed = parse_xdf_xml(&xdf_with_stride_at(-64, 0x48)).expect("parse");
    let table = &parsed.tables[0];

    let mut rom = vec![0u8; 0x100];
    let put = |rom: &mut Vec<u8>, at: usize, v: u16| {
        let b = v.to_be_bytes();
        rom[at] = b[0];
        rom[at + 1] = b[1];
    };
    // The declared address is row 0; a negative stride walks backwards from it.
    // So row 0 is at 0x48 and row 1 is 8 bytes earlier, at 0x40.
    put(&mut rom, 0x48, 1000);
    put(&mut rom, 0x4A, 2000);
    put(&mut rom, 0x40, 3000);
    put(&mut rom, 0x42, 4000);

    let matrix = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, table)
        .expect("extract failed");
    assert_eq!(
        matrix,
        vec![vec![1000.0, 2000.0], vec![3000.0, 4000.0]],
        "a negative stride must walk backwards from the declared address"
    );
}
