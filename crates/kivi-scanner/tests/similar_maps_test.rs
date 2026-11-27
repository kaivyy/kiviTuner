use kivi_defs::xdf::parse_xdf_full;
use kivi_scanner::similar_maps::{
    discover_parallel_map_groups, find_migrated_map_across_roms, find_similar_maps_in_rom,
    transfer_map_delta, DeltaTransferMode, SimilarMapOptions, SimilarMapSpec,
};

const DB: &str = "/root/ecu_master_database";

fn write_u16_le_table(rom: &mut [u8], addr: usize, rows: usize, cols: usize, vals: &[u16]) {
    assert_eq!(vals.len(), rows * cols);
    for (i, &v) in vals.iter().enumerate() {
        let bytes = v.to_le_bytes();
        rom[addr + i * 2] = bytes[0];
        rom[addr + i * 2 + 1] = bytes[1];
    }
}

#[test]
fn finds_exact_and_scaled_similar_maps_and_clusters_parallel_groups() {
    let rows = 4;
    let cols = 6;
    let base: Vec<u16> = (0..(rows * cols))
        .map(|i| 1000 + (i as u16) * 180 + ((i % cols) as u16) * 40)
        .collect();
    let scaled_5pct: Vec<u16> = base
        .iter()
        .map(|&v| ((v as f64) * 1.05).round() as u16)
        .collect();
    let unrelated: Vec<u16> = (0..(rows * cols))
        .map(|i| 60000 - (i as u16) * 1400)
        .collect();

    let mut rom = vec![0xFFu8; 4096];
    for (idx, b) in rom.iter_mut().enumerate() {
        if !(0x100..0xF00).contains(&idx) {
            *b = ((idx * 37) & 0xFF) as u8;
        }
    }
    write_u16_le_table(&mut rom, 0x200, rows, cols, &base);
    write_u16_le_table(&mut rom, 0x600, rows, cols, &base);
    write_u16_le_table(&mut rom, 0xA00, rows, cols, &scaled_5pct);
    write_u16_le_table(&mut rom, 0xC00, rows, cols, &unrelated);

    let source = SimilarMapSpec {
        address: 0x200,
        rows,
        cols,
        cell_bits: 16,
        little_endian: true,
        is_signed: false,
    };

    let hits = find_similar_maps_in_rom(&rom, &source, &SimilarMapOptions::default());
    assert!(hits.iter().any(|h| h.address == 0x600 && h.is_exact_copy));
    assert!(hits
        .iter()
        .any(|h| h.address == 0xA00 && h.pearson_r > 0.999 && !h.is_exact_copy));
    assert!(hits.iter().all(|h| h.address != 0x200));
    assert!(hits.iter().all(|h| h.address != 0xC00));

    let candidates = vec![
        source,
        SimilarMapSpec { address: 0x600, ..source },
        SimilarMapSpec { address: 0xA00, ..source },
        SimilarMapSpec { address: 0xC00, ..source },
    ];
    let groups = discover_parallel_map_groups(&rom, &candidates, 0.95, 0.15);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].anchor_address, 0x200);
    assert_eq!(groups[0].member_addresses, vec![0x200, 0x600, 0xA00]);
    assert!(groups[0].min_pearson_r > 0.999);
}

#[test]
fn migrates_shifted_map_across_roms_and_transfers_deltas() {
    let rows = 4;
    let cols = 5;
    let vals: Vec<u16> = (0..(rows * cols))
        .map(|i| 500 + (i as u16) * 210)
        .collect();

    let mut rom_a = vec![0xAAu8; 4096];
    let mut rom_b = vec![0x55u8; 4096];
    write_u16_le_table(&mut rom_a, 0x300, rows, cols, &vals);
    write_u16_le_table(&mut rom_b, 0x300 + 0x240, rows, cols, &vals);

    let spec = SimilarMapSpec {
        address: 0x300,
        rows,
        cols,
        cell_bits: 16,
        little_endian: true,
        is_signed: false,
    };
    let migrated =
        find_migrated_map_across_roms(&rom_a, &rom_b, &spec, &SimilarMapOptions::default());
    assert_eq!(migrated.len(), 1);
    assert_eq!(migrated[0].address, 0x540);
    assert!(migrated[0].is_exact_copy);

    let src_orig = vec![vec![100.0, 200.0], vec![0.0, 400.0]];
    let src_tuned = vec![vec![110.0, 220.0], vec![15.0, 440.0]];
    let tgt_orig = vec![vec![150.0, 300.0], vec![0.0, 500.0]];

    let abs = transfer_map_delta(&src_orig, &src_tuned, &tgt_orig, DeltaTransferMode::Absolute)
        .expect("abs");
    assert_eq!(abs, src_tuned);

    let add =
        transfer_map_delta(&src_orig, &src_tuned, &tgt_orig, DeltaTransferMode::AdditiveDelta)
            .expect("add");
    assert_eq!(add, vec![vec![160.0, 320.0], vec![15.0, 540.0]]);

    let pct = transfer_map_delta(
        &src_orig,
        &src_tuned,
        &tgt_orig,
        DeltaTransferMode::PercentageDelta,
    )
    .expect("pct");
    assert!((pct[0][0] - 165.0).abs() < 1e-9);
    assert!((pct[0][1] - 330.0).abs() < 1e-9);
    // Near-zero source falls back cleanly to additive delta (+15.0).
    assert!((pct[1][0] - 15.0).abs() < 1e-9);
    assert!((pct[1][1] - 550.0).abs() < 1e-9);
}

#[test]
fn real_suzuki_cross_revision_map_migration_achieves_high_correlation() {
    let rom_nc0_path = format!("{DB}/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSNC0.bin");
    let rom_jb0_path = format!("{DB}/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSJB0.bin");
    let xdf_nc0_path = format!("{DB}/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSNC0.xdf");
    if !std::path::Path::new(&rom_nc0_path).exists()
        || !std::path::Path::new(&rom_jb0_path).exists()
        || !std::path::Path::new(&xdf_nc0_path).exists()
    {
        return;
    }

    let rom_nc0 = std::fs::read(&rom_nc0_path).expect("read nc0");
    let rom_jb0 = std::fs::read(&rom_jb0_path).expect("read jb0");
    let xdf = parse_xdf_full(&std::fs::read_to_string(&xdf_nc0_path).expect("xdf")).expect("parse");

    // Pick a genuine non-constant 16-bit 2-D map (>= 8x8) from 5JCZSNC0.xdf.
    let target_map = xdf
        .maps
        .iter()
        .find(|m| {
            let b = m.dtype.byte_size();
            m.rows >= 8
                && m.cols >= 8
                && m.major_stride_bits == 0
                && matches!(b, 1 | 2)
                && m.address + m.rows * m.cols * b <= rom_nc0.len()
        })
        .expect("real 2D map in 5JCZSNC0.xdf");

    let spec = SimilarMapSpec {
        address: target_map.address as usize,
        rows: target_map.rows,
        cols: target_map.cols,
        cell_bits: (target_map.dtype.byte_size() * 8) as u16,
        little_endian: matches!(target_map.endian, kivi_core::types::Endian::Little),
        is_signed: false,
    };

    let hits = find_migrated_map_across_roms(
        &rom_nc0,
        &rom_jb0,
        &spec,
        &SimilarMapOptions {
            min_pearson_r: 0.90,
            max_nrmse: 0.20,
            ..Default::default()
        },
    );
    assert!(!hits.is_empty(), "expected real map migration hit in 5JCZSJB0");
    assert!(
        hits[0].pearson_r >= 0.95,
        "expected pearson_r >= 0.95 on real Suzuki pair, got {}",
        hits[0].pearson_r
    );
}
