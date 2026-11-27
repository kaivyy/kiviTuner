use kivi_scanner::blind_automap::{BlindAxis, BlindTable, SuggestedScaling};
use kivi_scanner::map_classifier::{
    classify_blind_table, classify_extrema_regions, AxisSelector, ClassificationProposal,
    ConditionGroup, ExtremaRegion, MapClassifierDb, MapCondition, MapRule, RuleGroup,
};

fn make_table(
    rows: usize,
    cols: usize,
    cell_bits: u16,
    matrix: Vec<Vec<f64>>,
    x_axis: Option<BlindAxis>,
    y_axis: Option<BlindAxis>,
) -> BlindTable {
    let mut raw_min = f64::INFINITY;
    let mut raw_max = f64::NEG_INFINITY;
    for r in &matrix {
        for &v in r {
            raw_min = raw_min.min(v);
            raw_max = raw_max.max(v);
        }
    }
    BlindTable {
        address: 0x10000,
        rows,
        cols,
        cell_bits,
        little_endian: true,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        raw_min,
        raw_max,
        matrix,
        x_axis,
        y_axis,
        suggested_scaling: SuggestedScaling {
            equation: "X".to_string(),
            unit: String::new(),
            decimals: 0,
            rationale: "test".to_string(),
        },
        label: "Test".to_string(),
        is_signed: false,
        is_float: false,
        is_curve: rows <= 1,
        detector_source: "test".to_string(),
    }
}

#[test]
fn extrema_regions_map_all_nine_zones_accurately() {
    let inner = vec![
        vec![50.0, 50.0, 50.0, 50.0],
        vec![50.0, 5.0, 95.0, 50.0],
        vec![50.0, 50.0, 50.0, 50.0],
    ];
    assert_eq!(
        classify_extrema_regions(&inner),
        (ExtremaRegion::Inner, ExtremaRegion::Inner)
    );

    let corners = vec![
        vec![1.0, 40.0, 50.0],
        vec![20.0, 30.0, 60.0],
        vec![70.0, 80.0, 99.0],
    ];
    assert_eq!(
        classify_extrema_regions(&corners),
        (ExtremaRegion::TopLeft, ExtremaRegion::BottomRight)
    );

    let edges = vec![
        vec![20.0, 1.0, 20.0],
        vec![20.0, 30.0, 99.0],
        vec![20.0, 40.0, 50.0],
    ];
    assert_eq!(
        classify_extrema_regions(&edges),
        (ExtremaRegion::TopEdge, ExtremaRegion::RightEdge)
    );
}

#[test]
fn starter_db_classifies_fuel_ignition_ve_rpm_compact_and_curves_with_priority() {
    let db = MapClassifierDb::builtin_starter_db();
    let json = db.to_json_pretty().expect("serialize");
    let reloaded = MapClassifierDb::from_json_str(&json).expect("roundtrip");
    assert_eq!(db, reloaded);

    // 16-bit fuel surface with RPM axis -> fuel_pulse_16bit (prio 90) must beat rpm_indexed_2d_map (75).
    let fuel_grid = vec![vec![500.0, 1200.0, 2000.0, 3000.0, 4000.0, 5200.0]; 6];
    let rpm_axis = BlindAxis {
        address: 0x0FF00,
        points: 6,
        is_16bit: true,
        little_endian: true,
        values: vec![1000.0, 2000.0, 3000.0, 4500.0, 6000.0, 7500.0],
    };
    let fuel_tbl = make_table(6, 6, 16, fuel_grid, Some(rpm_axis.clone()), None);
    let c_fuel = classify_blind_table(&fuel_tbl, None, &reloaded).expect("fuel match");
    assert_eq!(c_fuel.rule_id, "fuel_pulse_16bit");
    assert_eq!(c_fuel.unit, "ms");
    assert!(c_fuel.is_proposal);

    // 8-bit ignition map with peak at TopRight (not bottom-left).
    let mut ign_grid = vec![vec![20.0; 6]; 6];
    ign_grid[0][5] = 135.0;
    let ign_tbl = make_table(6, 6, 8, ign_grid, None, None);
    let c_ign = classify_blind_table(&ign_tbl, None, &reloaded).expect("ign match");
    assert_eq!(c_ign.rule_id, "ignition_timing_8bit");
    assert_eq!(c_ign.unit, "deg");

    // 8x8 16-bit VE/Load surface in 300..=2000.
    let mut ve_grid = vec![vec![400.0; 8]; 8];
    ve_grid[7][7] = 1400.0;
    let ve_tbl = make_table(8, 8, 16, ve_grid, None, None);
    let c_ve = classify_blind_table(&ve_tbl, None, &reloaded).expect("ve match");
    assert_eq!(c_ve.rule_id, "ve_or_load_surface_16bit");

    // 1-D curve (1x12).
    let curve_tbl = make_table(1, 12, 16, vec![(0..12).map(|i| 100.0 + i as f64 * 10.0).collect()], None, None);
    let c_curve = classify_blind_table(&curve_tbl, None, &reloaded).expect("curve match");
    assert_eq!(c_curve.rule_id, "calibration_curve_1d");
}

#[test]
fn pattern_ascii_and_boolean_or_and_groups_enforce_preconditions() {
    let custom = MapClassifierDb {
        schema_version: 1,
        groups: vec![RuleGroup {
            name: "Bosch ME7 Custom".to_string(),
            preconditions: vec![
                ConditionGroup {
                    any_of: vec![
                        MapCondition::RomAsciiContains("ME7.1".to_string()),
                        MapCondition::RomAsciiContains("MED9".to_string()),
                    ],
                },
                ConditionGroup {
                    any_of: vec![MapCondition::RomPatternPresent("DE AD ?? EF".to_string())],
                },
            ],
            rules: vec![MapRule {
                id: "custom_lamfa".to_string(),
                description: "Custom LAMFA rule".to_string(),
                condition_groups: vec![ConditionGroup {
                    any_of: vec![MapCondition::AxisBounds {
                        axis: AxisSelector::Y,
                        first_min: Some(400.0),
                        first_max: Some(1200.0),
                        last_min: Some(5000.0),
                        last_max: Some(8000.0),
                    }],
                }],
                proposal: ClassificationProposal {
                    role_name: "Suspected Driver Wish Lambda".to_string(),
                    category: "Fuel".to_string(),
                    unit: "lambda".to_string(),
                    factor: 0.0078125,
                    offset: 0.0,
                    decimals: 3,
                    priority: 100,
                },
            }],
        }],
    };

    let mut rom = b"BOSCH me7.1.1 TEST\x00\xDE\xAD\x42\xEF".to_vec();
    let y_axis = BlindAxis {
        address: 0x100,
        points: 4,
        is_16bit: true,
        little_endian: true,
        values: vec![800.0, 2000.0, 4000.0, 6500.0],
    };
    let tbl = make_table(4, 4, 8, vec![vec![128.0; 4]; 4], None, Some(y_axis));
    let hit = classify_blind_table(&tbl, Some(&rom), &custom).expect("custom rule matches");
    assert_eq!(hit.rule_id, "custom_lamfa");
    assert_eq!(hit.unit, "lambda");

    // Break fixed byte 0xDE at index 19 -> pattern precondition must no longer match.
    rom[19] = 0x00;
    assert!(classify_blind_table(&tbl, Some(&rom), &custom).is_none());
}
