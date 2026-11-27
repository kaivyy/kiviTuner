//! Rule-based semantic map classifier for blind calibration candidates.
//!
//! Assigns transparent `"Suspected ..."` role/unit/scaling proposals using
//! composable OR/AND condition groups, 3x3 extrema localization, axis bounds,
//! and optional ROM byte/ASCII signatures.

use serde::{Deserialize, Serialize};

use crate::blind_automap::BlindTable;
use crate::pattern_search::MaskedSignatureScanner;

/// 3x3 spatial region of a matrix where an extremum value occurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ExtremaRegion {
    TopLeft = 0,
    TopEdge = 1,
    TopRight = 2,
    LeftEdge = 3,
    Inner = 4,
    RightEdge = 5,
    BottomLeft = 6,
    BottomEdge = 7,
    BottomRight = 8,
}

impl ExtremaRegion {
    pub fn index(self) -> usize {
        self as usize
    }
}

fn axis_bucket(idx: usize, len: usize) -> usize {
    if len <= 1 {
        1
    } else if idx == 0 {
        0
    } else if idx + 1 == len {
        2
    } else {
        1
    }
}

fn region_from_buckets(row_b: usize, col_b: usize) -> ExtremaRegion {
    match row_b * 3 + col_b {
        0 => ExtremaRegion::TopLeft,
        1 => ExtremaRegion::TopEdge,
        2 => ExtremaRegion::TopRight,
        3 => ExtremaRegion::LeftEdge,
        4 => ExtremaRegion::Inner,
        5 => ExtremaRegion::RightEdge,
        6 => ExtremaRegion::BottomLeft,
        7 => ExtremaRegion::BottomEdge,
        _ => ExtremaRegion::BottomRight,
    }
}

/// Identifies the 3x3 spatial regions of `(min, max)` values inside `matrix`.
pub fn classify_extrema_regions(matrix: &[Vec<f64>]) -> (ExtremaRegion, ExtremaRegion) {
    let rows = matrix.len();
    let cols = matrix.first().map_or(0, Vec::len);
    if rows == 0 || cols == 0 {
        return (ExtremaRegion::Inner, ExtremaRegion::Inner);
    }

    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    let mut min_pos = (0usize, 0usize);
    let mut max_pos = (0usize, 0usize);

    for (r, row) in matrix.iter().enumerate() {
        for (c, &val) in row.iter().enumerate() {
            if !val.is_finite() {
                continue;
            }
            if val < min_v {
                min_v = val;
                min_pos = (r, c);
            }
            if val > max_v {
                max_v = val;
                max_pos = (r, c);
            }
        }
    }

    let min_region = region_from_buckets(
        axis_bucket(min_pos.0, rows),
        axis_bucket(min_pos.1, row_len_at(matrix, min_pos.0)),
    );
    let max_region = region_from_buckets(
        axis_bucket(max_pos.0, rows),
        axis_bucket(max_pos.1, row_len_at(matrix, max_pos.0)),
    );
    (min_region, max_region)
}

#[inline]
fn row_len_at(matrix: &[Vec<f64>], r: usize) -> usize {
    matrix.get(r).map_or(0, Vec::len)
}

/// Target axis selector for [`MapCondition::AxisBounds`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisSelector {
    X,
    Y,
}

/// Atomic classification predicate evaluated against a [`BlindTable`] and optional ROM bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum MapCondition {
    Dimensions {
        min_rows: Option<usize>,
        max_rows: Option<usize>,
        min_cols: Option<usize>,
        max_cols: Option<usize>,
    },
    CellBits(Vec<u16>),
    RawRange {
        min_lo: Option<f64>,
        min_hi: Option<f64>,
        max_lo: Option<f64>,
        max_hi: Option<f64>,
    },
    AxisBounds {
        axis: AxisSelector,
        first_min: Option<f64>,
        first_max: Option<f64>,
        last_min: Option<f64>,
        last_max: Option<f64>,
    },
    MaxPositionMask([bool; 9]),
    MinPositionMask([bool; 9]),
    RomPatternPresent(String),
    RomAsciiContains(String),
    IsCurve(bool),
}

/// OR-disjunction of conditions; all conditions inside [`any_of`] are ORed together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionGroup {
    pub any_of: Vec<MapCondition>,
}

/// Semantic role & physical display proposal attached when a [`MapRule`] matches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationProposal {
    pub role_name: String,
    pub category: String,
    pub unit: String,
    pub factor: f64,
    pub offset: f64,
    pub decimals: u8,
    pub priority: u32,
}

/// One classification rule: matches when every [`ConditionGroup`] in [`condition_groups`] holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapRule {
    pub id: String,
    pub description: String,
    pub condition_groups: Vec<ConditionGroup>,
    pub proposal: ClassificationProposal,
}

/// Named collection of rules guarded by group-level [`preconditions`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleGroup {
    pub name: String,
    pub preconditions: Vec<ConditionGroup>,
    pub rules: Vec<MapRule>,
}

/// Serializable rule database for semantic blind-map hints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapClassifierDb {
    pub schema_version: u32,
    pub groups: Vec<RuleGroup>,
}

/// Classification outcome emitted by [`classify_blind_table`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapClassification {
    pub rule_id: String,
    pub group_name: String,
    pub role_name: String,
    pub category: String,
    pub unit: String,
    pub equation: String,
    pub factor: f64,
    pub offset: f64,
    pub decimals: u8,
    pub min_region: ExtremaRegion,
    pub max_region: ExtremaRegion,
    pub is_proposal: bool,
    pub rationale: String,
}

fn check_bound_usize(val: usize, min_v: Option<usize>, max_v: Option<usize>) -> bool {
    min_v.is_none_or(|lo| val >= lo) && max_v.is_none_or(|hi| val <= hi)
}

fn check_bound_f64(val: f64, min_v: Option<f64>, max_v: Option<f64>) -> bool {
    min_v.is_none_or(|lo| val >= lo) && max_v.is_none_or(|hi| val <= hi)
}

fn ascii_contains_ignore_case(haystack: &[u8], needle: &str) -> bool {
    let nb = needle.as_bytes();
    if nb.is_empty() {
        return true;
    }
    if haystack.len() < nb.len() {
        return false;
    }
    haystack
        .windows(nb.len())
        .any(|w| w.eq_ignore_ascii_case(nb))
}

fn evaluate_condition(
    cond: &MapCondition,
    table: &BlindTable,
    rom: Option<&[u8]>,
    min_reg: ExtremaRegion,
    max_reg: ExtremaRegion,
) -> bool {
    match cond {
        MapCondition::Dimensions {
            min_rows,
            max_rows,
            min_cols,
            max_cols,
        } => {
            check_bound_usize(table.rows, *min_rows, *max_rows)
                && check_bound_usize(table.cols, *min_cols, *max_cols)
        }
        MapCondition::CellBits(bits) => bits.contains(&table.cell_bits),
        MapCondition::RawRange {
            min_lo,
            min_hi,
            max_lo,
            max_hi,
        } => {
            check_bound_f64(table.raw_min, *min_lo, *min_hi)
                && check_bound_f64(table.raw_max, *max_lo, *max_hi)
        }
        MapCondition::AxisBounds {
            axis,
            first_min,
            first_max,
            last_min,
            last_max,
        } => {
            let ax = match axis {
                AxisSelector::X => table.x_axis.as_ref(),
                AxisSelector::Y => table.y_axis.as_ref(),
            };
            let Some(ax) = ax else {
                return false;
            };
            let (Some(&first), Some(&last)) = (ax.values.first(), ax.values.last()) else {
                return false;
            };
            check_bound_f64(first, *first_min, *first_max)
                && check_bound_f64(last, *last_min, *last_max)
        }
        MapCondition::MaxPositionMask(mask) => mask[max_reg.index()],
        MapCondition::MinPositionMask(mask) => mask[min_reg.index()],
        MapCondition::RomPatternPresent(pat) => {
            let Some(bytes) = rom else {
                return false;
            };
            let Ok(parsed) = MaskedSignatureScanner::parse_pattern(pat) else {
                return false;
            };
            MaskedSignatureScanner::find_first(bytes, &parsed).is_some()
        }
        MapCondition::RomAsciiContains(text) => {
            if text.is_empty() {
                return true;
            }
            let Some(bytes) = rom else {
                return false;
            };
            ascii_contains_ignore_case(bytes, text)
        }
        MapCondition::IsCurve(expected) => {
            let is_curve = table.rows <= 1 && table.cols >= 2;
            is_curve == *expected
        }
    }
}

fn evaluate_groups(
    groups: &[ConditionGroup],
    table: &BlindTable,
    rom: Option<&[u8]>,
    min_reg: ExtremaRegion,
    max_reg: ExtremaRegion,
) -> bool {
    groups.iter().all(|g| {
        g.any_of.is_empty()
            || g.any_of
                .iter()
                .any(|c| evaluate_condition(c, table, rom, min_reg, max_reg))
    })
}

fn format_proposal_equation(factor: f64, offset: f64) -> String {
    if (factor - 1.0).abs() < 1e-12 && offset.abs() < 1e-12 {
        "X".to_string()
    } else if offset.abs() < 1e-12 {
        format!("X * {factor}")
    } else if offset > 0.0 {
        format!("X * {factor} + {offset}")
    } else {
        format!("X * {factor} - {}", -offset)
    }
}

impl MapClassifierDb {
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Returns a curated starter set of transparent `"Suspected ..."` heuristic rules.
    pub fn builtin_starter_db() -> Self {
        let not_solely_bottom_left = [true, true, true, true, true, true, false, true, true];
        Self {
            schema_version: 1,
            groups: vec![RuleGroup {
                name: "Builtin Generic ECU Heuristics".to_string(),
                preconditions: Vec::new(),
                rules: vec![
                    MapRule {
                        id: "fuel_pulse_16bit".to_string(),
                        description: "16-bit >=6x6 table with pulse-width magnitude >2000 and >=1000 span".to_string(),
                        condition_groups: vec![
                            ConditionGroup { any_of: vec![MapCondition::IsCurve(false)] },
                            ConditionGroup { any_of: vec![MapCondition::CellBits(vec![16])] },
                            ConditionGroup {
                                any_of: vec![MapCondition::Dimensions {
                                    min_rows: Some(6),
                                    max_rows: None,
                                    min_cols: Some(6),
                                    max_cols: None,
                                }],
                            },
                            ConditionGroup {
                                any_of: vec![MapCondition::RawRange {
                                    min_lo: Some(0.0),
                                    min_hi: Some(4000.0),
                                    max_lo: Some(2001.0),
                                    max_hi: None,
                                }],
                            },
                        ],
                        proposal: ClassificationProposal {
                            role_name: "Suspected Fuel / Injection Map".to_string(),
                            category: "Fuel & Injection".to_string(),
                            unit: "ms".to_string(),
                            factor: 0.001,
                            offset: 0.0,
                            decimals: 3,
                            priority: 90,
                        },
                    },
                    MapRule {
                        id: "ignition_timing_8bit".to_string(),
                        description: "8-bit >=6x6 spark/advance surface excluding bottom-left peak".to_string(),
                        condition_groups: vec![
                            ConditionGroup { any_of: vec![MapCondition::IsCurve(false)] },
                            ConditionGroup { any_of: vec![MapCondition::CellBits(vec![8])] },
                            ConditionGroup {
                                any_of: vec![MapCondition::Dimensions {
                                    min_rows: Some(6),
                                    max_rows: None,
                                    min_cols: Some(6),
                                    max_cols: None,
                                }],
                            },
                            ConditionGroup {
                                any_of: vec![MapCondition::RawRange {
                                    min_lo: Some(0.0),
                                    min_hi: Some(80.0),
                                    max_lo: Some(80.0),
                                    max_hi: Some(240.0),
                                }],
                            },
                            ConditionGroup {
                                any_of: vec![MapCondition::MaxPositionMask(not_solely_bottom_left)],
                            },
                        ],
                        proposal: ClassificationProposal {
                            role_name: "Suspected Ignition / Spark Map".to_string(),
                            category: "Ignition & Timing".to_string(),
                            unit: "deg".to_string(),
                            factor: 0.75,
                            offset: 0.0,
                            decimals: 1,
                            priority: 85,
                        },
                    },
                    MapRule {
                        id: "rpm_indexed_2d_map".to_string(),
                        description: "2-D calibration table indexed by an attached RPM-scale axis".to_string(),
                        condition_groups: vec![
                            ConditionGroup { any_of: vec![MapCondition::IsCurve(false)] },
                            ConditionGroup {
                                any_of: vec![
                                    MapCondition::AxisBounds {
                                        axis: AxisSelector::X,
                                        first_min: None,
                                        first_max: None,
                                        last_min: Some(4000.0),
                                        last_max: Some(20000.0),
                                    },
                                    MapCondition::AxisBounds {
                                        axis: AxisSelector::Y,
                                        first_min: None,
                                        first_max: None,
                                        last_min: Some(4000.0),
                                        last_max: Some(20000.0),
                                    },
                                ],
                            },
                        ],
                        proposal: ClassificationProposal {
                            role_name: "Suspected RPM-Indexed Calibration Map".to_string(),
                            category: "Engine Calibration".to_string(),
                            unit: String::new(),
                            factor: 1.0,
                            offset: 0.0,
                            decimals: 1,
                            priority: 75,
                        },
                    },
                    MapRule {
                        id: "ve_or_load_surface_16bit".to_string(),
                        description: "16-bit >=8x8 surface in moderate range 300..=2000".to_string(),
                        condition_groups: vec![
                            ConditionGroup { any_of: vec![MapCondition::IsCurve(false)] },
                            ConditionGroup { any_of: vec![MapCondition::CellBits(vec![16])] },
                            ConditionGroup {
                                any_of: vec![MapCondition::Dimensions {
                                    min_rows: Some(8),
                                    max_rows: None,
                                    min_cols: Some(8),
                                    max_cols: None,
                                }],
                            },
                            ConditionGroup {
                                any_of: vec![MapCondition::RawRange {
                                    min_lo: None,
                                    min_hi: None,
                                    max_lo: Some(300.0),
                                    max_hi: Some(2000.0),
                                }],
                            },
                        ],
                        proposal: ClassificationProposal {
                            role_name: "Suspected VE / Load / Lambda Surface".to_string(),
                            category: "Airflow & Load".to_string(),
                            unit: "raw".to_string(),
                            factor: 1.0,
                            offset: 0.0,
                            decimals: 1,
                            priority: 65,
                        },
                    },
                    MapRule {
                        id: "compact_limiter_or_correction_2d".to_string(),
                        description: "Compact 4..6 x 4..8 2-D correction or limiter map".to_string(),
                        condition_groups: vec![
                            ConditionGroup { any_of: vec![MapCondition::IsCurve(false)] },
                            ConditionGroup {
                                any_of: vec![MapCondition::Dimensions {
                                    min_rows: Some(4),
                                    max_rows: Some(6),
                                    min_cols: Some(4),
                                    max_cols: Some(8),
                                }],
                            },
                        ],
                        proposal: ClassificationProposal {
                            role_name: "Suspected Correction / Limiter Map".to_string(),
                            category: "Limiters & Corrections".to_string(),
                            unit: String::new(),
                            factor: 1.0,
                            offset: 0.0,
                            decimals: 1,
                            priority: 55,
                        },
                    },
                    MapRule {
                        id: "calibration_curve_1d".to_string(),
                        description: "1-D lookup curve with at least 8 points".to_string(),
                        condition_groups: vec![
                            ConditionGroup { any_of: vec![MapCondition::IsCurve(true)] },
                            ConditionGroup {
                                any_of: vec![MapCondition::Dimensions {
                                    min_rows: Some(1),
                                    max_rows: Some(1),
                                    min_cols: Some(8),
                                    max_cols: None,
                                }],
                            },
                        ],
                        proposal: ClassificationProposal {
                            role_name: "Suspected 1-D Calibration / Compensation Curve".to_string(),
                            category: "Curves & Compensation".to_string(),
                            unit: String::new(),
                            factor: 1.0,
                            offset: 0.0,
                            decimals: 1,
                            priority: 50,
                        },
                    },
                ],
            }],
        }
    }
}

/// Evaluates `db` against `table` (and optional full `rom` bytes) and returns the
/// highest-priority matching semantic proposal.
pub fn classify_blind_table(
    table: &BlindTable,
    rom: Option<&[u8]>,
    db: &MapClassifierDb,
) -> Option<MapClassification> {
    let (min_region, max_region) = classify_extrema_regions(&table.matrix);
    let mut best: Option<(u32, &str, &str, &MapRule)> = None;

    for group in &db.groups {
        if !evaluate_groups(&group.preconditions, table, rom, min_region, max_region) {
            continue;
        }
        for rule in &group.rules {
            if !evaluate_groups(&rule.condition_groups, table, rom, min_region, max_region) {
                continue;
            }
            let replace = match best {
                None => true,
                Some((best_prio, _, best_id, _)) => {
                    rule.proposal.priority > best_prio
                        || (rule.proposal.priority == best_prio && rule.id.as_str() < best_id)
                }
            };
            if replace {
                best = Some((rule.proposal.priority, &group.name, &rule.id, rule));
            }
        }
    }

    let (_prio, group_name, _rule_id, rule) = best?;
    let p = &rule.proposal;
    Some(MapClassification {
        rule_id: rule.id.clone(),
        group_name: group_name.to_string(),
        role_name: p.role_name.clone(),
        category: p.category.clone(),
        unit: p.unit.clone(),
        equation: format_proposal_equation(p.factor, p.offset),
        factor: p.factor,
        offset: p.offset,
        decimals: p.decimals,
        min_region,
        max_region,
        is_proposal: true,
        rationale: format!(
            "{} (matched heuristic rule '{}' in '{}')",
            rule.description, rule.id, group_name
        ),
    })
}
