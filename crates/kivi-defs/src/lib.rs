
// Style lints relaxed for the verbatim port; correctness lints stay denied.
#![allow(
    clippy::manual_div_ceil,
    clippy::needless_range_loop,
    clippy::manual_range_contains,
    clippy::derivable_impls,
    clippy::manual_is_multiple_of,
    clippy::should_implement_trait,
    clippy::inherent_to_string,
    clippy::op_ref
)]

pub mod hexdump;
pub mod gauges;
pub mod catalog;
pub mod definition;
pub mod ast;
pub mod expr;
pub mod float_map;
pub mod interp;
pub mod projection;
pub mod kivi_json;
pub mod map_ops;
pub mod model;
pub mod patch;
pub mod smoothing;
pub mod ecu_xml;
pub mod ecu_ini;
pub mod twin_maps;
pub mod xdf;
pub mod xdf_export;

pub use expr::{eval_scaling, evaluate_scaling, invert_scaling, ExprError};
pub use interp::{
    bilinear_2d, catmull_rom_cubic_1d, lerp_1d, lookup_1d_curve, lookup_2d_map, search_interval,
    AxisInterval, InterpError,
};
pub use kivi_json::{from_json_str, maps_from_json, maps_to_json, to_json_pretty, to_json_string};
pub use model::{AxisDef, ConstantDef, Definitions, FlagDef, MapDef, Scaling};
pub use smoothing::{
    find_safest_row_to_remove, gaussian_smooth_3x3, laplacian_curvature_2d, spatial_smooth_2d,
    RowReductionResult, SmoothError,
};
pub use xdf::{parse_xdf, parse_xdf_full, XdfError};
