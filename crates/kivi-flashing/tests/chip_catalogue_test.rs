use kivi_flashing::chip_catalogue::{
    chip_catalogue_count, find_chips, find_chips_by_jedec,
};

#[test]
fn embedded_batronix_catalogue_loads_all_6742_entries() {
    assert_eq!(chip_catalogue_count(), 6742);
    assert!(find_chips("").is_empty());
    assert!(find_chips("   ").is_empty());
}

#[test]
fn finds_common_ecu_flash_and_eprom_parts_by_name_and_jedec_id() {
    let am27 = find_chips("AM27128A");
    assert!(!am27.is_empty());
    assert!(am27.iter().any(|c| {
        c.manufacturer == "AMD" && c.jedec_id_pairs == vec![(0x0, 0x1), (0x1, 0x89)]
    }));

    let f400 = find_chips("29F400");
    assert!(f400.len() >= 4, "expected multiple 29F400 variants");
    let f800 = find_chips("AM29F800BB");
    assert!(!f800.is_empty(), "expected AM29F800BB entries");

    // 8-bit EPROM JEDEC: AMD 0x01, AM27128A 0x89.
    let by_jedec_8 = find_chips_by_jedec(0x01, 0x89);
    assert!(by_jedec_8.iter().any(|c| c.chip_name.contains("AM27128A")));

    // 16-bit Flash JEDEC: AMD 0x01, AM29F800BB 0x2258.
    let by_jedec_16 = find_chips_by_jedec(0x01, 0x2258);
    assert!(by_jedec_16.iter().any(|c| c.chip_name.contains("AM29F800BB")));

    // Second-pair at register 0x02: AMD AM29F400BB (0x0: 0x1, 0x2: 0xAB).
    let by_reg2 = find_chips_by_jedec(0x01, 0xAB);
    assert!(by_reg2.iter().any(|c| c.chip_name.contains("AM29F400BB")));
}
