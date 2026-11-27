use kivi_core::types::{DType, Endian};
use kivi_defs::model::{MapDef, Scaling};
use kivi_protocol::{model::ConnectionState, simulator::{SimMode, VirtualEcuSimulator}};
use kivituner::commands::*;

    #[test]
    fn test_load_bin_data_and_save() {
        let state = AppState::default();
        let sample = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let hashes = load_bin_data_inner(&sample, &state).expect("load failed");
        assert!(!hashes.md5.is_empty());
        assert!(!hashes.sha256.is_empty());

        let saved = save_bin_data_inner(&state).expect("save failed");
        assert_eq!(saved, sample);
    }

    #[test]
    fn test_parse_xdf_content() {
        let state = AppState::default();
        let xdf = r#"<XDFFORMAT version="1.60">
            <XDFTABLE uniqueid="0x100" flags="0x0">
                <title>Primary Fuel Map</title>
                <description>Main fuel injection base table</description>
                <CATEGORYMEM index="0" category="1" />
                <XDFAXIS id="x" uniqueid="0x0">
                    <EMBEDDEDDATA mmedtypeflags="0x02" mmedaddress="0x1000" mmedelementsizebits="16" />
                </XDFAXIS>
                <XDFAXIS id="y" uniqueid="0x0">
                    <EMBEDDEDDATA mmedtypeflags="0x02" mmedaddress="0x2000" mmedelementsizebits="16" />
                </XDFAXIS>
                <XDFAXIS id="z">
                    <EMBEDDEDDATA mmedtypeflags="0x00" mmedaddress="0x4000" mmedelementsizebits="8" />
                    <decimalpl>2</decimalpl>
                    <DALINK index="0">
                        <MATH equation="X * 0.5">
                            <VAR id="X" />
                        </MATH>
                    </DALINK>
                </XDFAXIS>
            </XDFTABLE>
        </XDFFORMAT>"#;

        let defs = parse_xdf_content_inner(xdf, &state).expect("xdf parse failed");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "Primary Fuel Map");
        assert_eq!(defs[0].address, 0x4000);
    }

    #[test]
    fn test_extract_and_update_map_matrix() {
        let state = AppState::default();
        // 2x2 table at address 0x10 with 8-bit values: [10, 20, 30, 40]
        let mut rom = vec![0u8; 64];
        rom[0x10] = 10;
        rom[0x11] = 20;
        rom[0x12] = 30;
        rom[0x13] = 40;
        load_bin_data_inner(&rom, &state).expect("load failed");

        let map_def = MapDef {
            id: "fuel_table".to_string(),
            name: "Fuel Table".to_string(),
            category: "Fuel".to_string(),
            address: 0x10,
            rows: 2,
            cols: 2,
            dtype: DType::U8,
            endian: Endian::Little,
            column_major: false,
            major_stride_bits: 0,
            minor_stride_bits: 0,
            scaling: Scaling {
                equation: "X * 2".to_string(),
                unit: "ms".to_string(),
                decimals: 1,
            },
            x_axis: None,
            y_axis: None,
        };

        // Extract scaled matrix: [10*2, 20*2], [30*2, 40*2] -> [[20, 40], [60, 80]]
        let matrix = extract_map_matrix_inner(&map_def, &state).expect("extract failed");
        assert_eq!(matrix, vec![vec![20.0, 40.0], vec![60.0, 80.0]]);

        // Update matrix with new scaled values: [[22.0, 44.0], [66.0, 88.0]]
        // Inverse scaling: raw = val / 2 -> [[11, 22], [33, 44]]
        let new_matrix = vec![vec![22.0, 44.0], vec![66.0, 88.0]];
        update_map_matrix_inner(&map_def, &new_matrix, &state).expect("update failed");

        // Verify updated raw bytes in rom
        let saved = save_bin_data_inner(&state).expect("save failed");
        assert_eq!(saved[0x10], 11);
        assert_eq!(saved[0x11], 22);
        assert_eq!(saved[0x12], 33);
        assert_eq!(saved[0x13], 44);

        // Re-extract and verify
        let re_extracted = extract_map_matrix_inner(&map_def, &state).expect("re-extract failed");
        assert_eq!(re_extracted, vec![vec![22.0, 44.0], vec![66.0, 88.0]]);
    }

    #[test]
    fn test_checksum_verify_and_fix() {
        let state = AppState::default();
        let mut rom = vec![1u8; 1024];
        // At 1024 ones, sum mod 256 is 1024 % 256 = 0, so checksum happens to be valid if sum % 256 == 0.
        // Let's make it invalid:
        rom[0] = 5; // sum mod 256 = 4 != 0
        load_bin_data_inner(&rom, &state).expect("load failed");

        let is_valid = verify_checksum_keihin_inner(&state).expect("verify failed");
        assert!(!is_valid);

        let fixed = fix_checksum_keihin_inner(&state).expect("fix failed");
        assert!(fixed);

        let is_valid_after = verify_checksum_keihin_inner(&state).expect("verify failed");
        assert!(is_valid_after);
    }

    #[test]
    fn test_compare_rom_binaries_and_convert() {
        let base = vec![0u8; 128];
        let mut target = vec![0u8; 128];
        // Introduce a 16-byte difference at offset 0x20
        target[0x20..0x30].fill(42);

        let diffs = compare_rom_binaries(base, target, 4).expect("diff failed");
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].start_addr, 0x20);
        assert_eq!(diffs[0].end_addr, 0x30);
        assert_eq!(diffs[0].byte_len, 16);

        let map_def = convert_diff_to_map(diffs[0].clone()).expect("conversion failed");
        assert_eq!(map_def.address, 0x20);
    }

    #[test]
    fn test_scan_rom_heuristics() {
        let mut rom = vec![0u8; 256];
        // Synthetic monotonic axis at 0x10 (8 bytes)
        for i in 0..8 {
            rom[0x10 + i] = (i * 20) as u8;
        }

        let candidates = scan_rom_heuristics(rom, 50).expect("scan failed");
        let _ = candidates.len();
    }

    #[tokio::test]
    async fn test_telemetry_commands_unit_flow() {
        let state = AppState::default();

        let drivers = list_ecu_drivers_inner(&state).expect("list drivers failed");
        assert!(drivers.iter().all(|d| d.id != "virtual_bench" && !d.is_simulator));

        let status = get_connection_status_inner(&state).await.expect("status failed");
        assert_eq!(status, ConnectionState::Disconnected);

        let sim = std::sync::Arc::new(VirtualEcuSimulator::new(SimMode::IdleWarmup));
        let id = state
            .telemetry
            .connect_driver(sim.clone(), None)
            .await
            .expect("connect failed");
        assert_eq!(id.ecu_id, "SIM-BENCH-01");

        let status_connected = get_connection_status_inner(&state).await.expect("status failed");
        assert!(matches!(status_connected, ConnectionState::Connected { .. }));

        sim.set_mode(SimMode::DynoPull);

        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        let frame = get_latest_telemetry_inner(&state)
            .await
            .expect("telemetry fetch failed");
        assert!(frame.is_some());

        disconnect_ecu_inner(None, &state)
            .await
            .expect("disconnect failed");

        let status_disconnected = get_connection_status_inner(&state).await.expect("status failed");
        assert_eq!(status_disconnected, ConnectionState::Disconnected);
    }
