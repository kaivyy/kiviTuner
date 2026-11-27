//! XDF export (Zero-XDF Z5).
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine::export_xdf_xml`.
//! Emits a the reference implementation-compatible XDF 1.60 document from discovered tables and
//! limiters. Pure string building; no I/O.

use std::fmt::Write as _;

/// Serialises discovered tables and limiters into an XDF 1.60 document.
pub fn export_xdf_xml(

    title_header: &str,
    tables: &[crate::zero_xdf_scan::DiscoveredFullTable],
    limiters: &[crate::limiter_scan::DiscoveredLimiter],
) -> String {
    let mut xdf = String::with_capacity(16384);
    let _ = writeln!(xdf, "<!-- XDF Calibration Definition Generated Autonomously by ECU Engine (Zero-XDF Engine) -->");
    let _ = writeln!(xdf, "<XDFFORMAT version=\"1.60\">");
    let _ = writeln!(xdf, "  <XDFHEADER>");
    let _ = writeln!(xdf, "    <flags>0x1</flags>");
    let _ = writeln!(xdf, "    <deftitle>{}</deftitle>", title_header);
    let _ = writeln!(xdf, "    <author>ECU Engine Autonomous Zero-XDF</author>");
    let _ = writeln!(xdf, "  </XDFHEADER>");
    xdf.push('\n');
    let _ = writeln!(xdf, "  <!-- Hierarchical 2-Tier Categories -->");
    let _ = writeln!(xdf, "  <CATEGORY index=\"0\" name=\"[TIER 1] Ignition Advance Maps\" />");
    let _ = writeln!(xdf, "  <CATEGORY index=\"1\" name=\"[TIER 1] Fuel &amp; VE Air-Charge Maps\" />");
    let _ = writeln!(xdf, "  <CATEGORY index=\"2\" name=\"[TIER 1] Boost Target &amp; Torque Limiter Maps\" />");
    let _ = writeln!(xdf, "  <CATEGORY index=\"3\" name=\"[TIER 1] RPM Limiters &amp; Critical 1D Scalars\" />");
    let _ = writeln!(xdf, "  <CATEGORY index=\"4\" name=\"[TIER 2] OEM Extended / Hidden Gem Discovery\" />");
    xdf.push('\n');

    // Export 1D Limiters / Constants
    for (i, lim) in limiters.iter().enumerate() {
        let _ = writeln!(xdf, "  <XDFCONSTANT uniqueid=\"0x{:04X}\">", 0x1000 + i);
        let _ = writeln!(xdf, "    <title>[TIER 1] {} (0x{:06X})</title>", lim.limiter_type, lim.address);
        let _ = writeln!(xdf, "    <description>Discovered Rev Limiter: {:.1} RPM</description>", lim.converted_rpm);
        let _ = writeln!(xdf, "    <CATEGORYMEM index=\"0\" category=\"3\" />");
        let _ = writeln!(xdf, "    <EMBEDDEDDATA mmedaddress=\"0x{:06X}\" mmedelementsizebits=\"16\" mmedtypeflags=\"0x02\" />", lim.address);
        let _ = writeln!(xdf, "    <units>RPM</units>");
        let _ = writeln!(xdf, "    <MATH equation=\"X\" />");
        let _ = writeln!(xdf, "  </XDFCONSTANT>");
    }

    // Export 3D Tables with X and Y Axes
    for (i, tbl) in tables.iter().enumerate() {
        let (cat_idx, type_str) = match tbl.semantic_type {
            crate::zero_xdf_scan::SemanticMapType::IgnitionAdvance => (0, "Ignition Advance Map"),
            crate::zero_xdf_scan::SemanticMapType::FuelAirChargeVE => (1, "Fuel / VE Air-Charge Map"),
            crate::zero_xdf_scan::SemanticMapType::BoostTorqueLimiter => (2, "Boost / Torque Limit Map"),
            crate::zero_xdf_scan::SemanticMapType::TargetLambdaAFR => (1, "Target Lambda / AFR Map"),
            _ => (4, "OEM Extended Calibration Map"),
        };
        let is_tier1 = tbl.confidence_score >= 0.85 && tbl.x_axis.is_some() && tbl.y_axis.is_some() && cat_idx != 4;
        let final_cat = if is_tier1 { cat_idx } else { 4 };
        let tier_prefix = if is_tier1 { "[TIER 1]" } else { "[TIER 2]" };

        let (bits, flags) = match tbl.data_format {
            crate::zero_xdf_scan::MapDataBits::Bits8 => (8, "0x01"),
            crate::zero_xdf_scan::MapDataBits::Bits16(crate::zero_xdf_scan::Endianness::LittleEndian) => (16, "0x02"),
            crate::zero_xdf_scan::MapDataBits::Bits16(crate::zero_xdf_scan::Endianness::BigEndian) => (16, "0x00"),
            crate::zero_xdf_scan::MapDataBits::Bits32(crate::zero_xdf_scan::Endianness::LittleEndian) => (32, "0x02"),
            crate::zero_xdf_scan::MapDataBits::Bits32(crate::zero_xdf_scan::Endianness::BigEndian) => (32, "0x00"),
        };

        let _ = writeln!(xdf, "  <XDFTABLE uniqueid=\"0x{:04X}\">", 0x2000 + i);
        let _ = writeln!(xdf, "    <title>{} {} (0x{:06X} - {}x{})</title>", tier_prefix, type_str, tbl.z_address, tbl.rows, tbl.cols);
        let _ = writeln!(xdf, "    <description>Smoothness: {:.3}, Confidence: {:.1}%</description>", tbl.smoothness, tbl.confidence_score * 100.0);
        let _ = writeln!(xdf, "    <CATEGORYMEM index=\"0\" category=\"{}\" />", final_cat);

        // X-Axis (Columns)
        let _ = writeln!(xdf, "    <XDFAXIS id=\"x\">");
        let _ = writeln!(xdf, "      <indexcount>{}</indexcount>", tbl.cols);
        if let Some(ref x_ax) = tbl.x_axis {
            let x_flags = if x_ax.endianness == crate::zero_xdf_scan::Endianness::LittleEndian { "0x02" } else { "0x00" };
            let x_bits = if x_ax.is_16bit { 16 } else { 8 };
            let _ = writeln!(xdf, "      <EMBEDDEDDATA mmedaddress=\"0x{:06X}\" mmedelementsizebits=\"{}\" mmedtypeflags=\"{}\" />", x_ax.address, x_bits, x_flags);
            let _ = writeln!(xdf, "      <units>{}</units>", x_ax.estimated_unit);
        }
        let _ = writeln!(xdf, "      <MATH equation=\"X\" />");
        let _ = writeln!(xdf, "    </XDFAXIS>");

        // Y-Axis (Rows)
        let _ = writeln!(xdf, "    <XDFAXIS id=\"y\">");
        let _ = writeln!(xdf, "      <indexcount>{}</indexcount>", tbl.rows);
        if let Some(ref y_ax) = tbl.y_axis {
            let y_flags = if y_ax.endianness == crate::zero_xdf_scan::Endianness::LittleEndian { "0x02" } else { "0x00" };
            let y_bits = if y_ax.is_16bit { 16 } else { 8 };
            let _ = writeln!(xdf, "      <EMBEDDEDDATA mmedaddress=\"0x{:06X}\" mmedelementsizebits=\"{}\" mmedtypeflags=\"{}\" />", y_ax.address, y_bits, y_flags);
            let _ = writeln!(xdf, "      <units>{}</units>", y_ax.estimated_unit);
        }
        let _ = writeln!(xdf, "      <MATH equation=\"X\" />");
        let _ = writeln!(xdf, "    </XDFAXIS>");

        // Z-Axis (Data Cells)
        let _ = writeln!(xdf, "    <XDFAXIS id=\"z\">");
        let _ = writeln!(xdf, "      <EMBEDDEDDATA mmedaddress=\"0x{:06X}\" mmedelementsizebits=\"{}\" mmedtypeflags=\"{}\" mmedrowcount=\"{}\" mmedcolcount=\"{}\" />",
            tbl.z_address, bits, flags, tbl.rows, tbl.cols);
        let _ = writeln!(xdf, "      <MATH equation=\"X\" />");
        let _ = writeln!(xdf, "    </XDFAXIS>");

        let _ = writeln!(xdf, "  </XDFTABLE>");
    }

    let _ = writeln!(xdf, "</XDFFORMAT>");
    xdf
}