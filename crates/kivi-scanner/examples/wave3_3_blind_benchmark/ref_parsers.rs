use std::collections::BTreeMap;
use std::path::PathBuf;
use kivi_core::address_translation::{cpu_to_raw, translate_me7, Me7Translation};
use kivi_defs::xdf::parse_xdf_full;
use kivi_scanner::a2l_parse::{parse_a2l, A2lCharacteristicType};
use kivi_scanner::damos_parse::parse_damos;
use super::corpus::*;

pub(crate) fn span_is_varying(rom: &[u8], addr: usize, rows: usize, cols: usize, elem_bytes: usize) -> bool {
    let span = rows.saturating_mul(cols).saturating_mul(elem_bytes);
    if span == 0 || addr.saturating_add(span) > rom.len() {
        return false;
    }
    let slice = &rom[addr..addr + span];
    let first = slice[0];
    slice.iter().any(|&b| b != first)
}

pub(crate) fn dedup_ref_items(items: Vec<RefItem>) -> Vec<RefItem> {
    let mut map: BTreeMap<(usize, bool), RefItem> = BTreeMap::new();
    for item in items {
        map.entry((item.address, item.is_2d))
            .and_modify(|prev| {
                if item.rows * item.cols > prev.rows * prev.cols {
                    *prev = item.clone();
                } else {
                    prev.has_ref_axes |= item.has_ref_axes;
                }
            })
            .or_insert(item);
    }
    map.into_values().collect()
}

pub(crate) fn parse_reference_file(spec: &RefSpec, rom: &[u8]) -> ParsedRefSummary {
    let full = PathBuf::from(DB).join(spec.rel_path);
    let raw_bytes = std::fs::read(&full)
        .unwrap_or_else(|e| panic!("FATAL: cannot read ref '{}': {e}", full.display()));
    let text = String::from_utf8_lossy(&raw_bytes);
    let rom_len = rom.len();

    match spec.format {
        RefFormat::Xdf => {
            let defs = parse_xdf_full(&text)
                .unwrap_or_else(|e| panic!("FATAL: parse XDF '{}': {e}", full.display()));
            let raw_items = defs.maps.len();
            let mut oob = 0usize;
            let mut kept = Vec::new();
            for m in defs.maps {
                let is_2d = m.rows > 1 && m.cols > 1;
                let is_1d = (m.rows == 1 || m.cols == 1) && m.rows.max(m.cols) >= 4;
                if !is_2d && !is_1d {
                    continue;
                }
                let elem_bytes = m.dtype.byte_size().max(1);
                let span = m.rows.saturating_mul(m.cols).saturating_mul(elem_bytes);
                if span == 0 || m.address.saturating_add(span) > rom_len {
                    oob += 1;
                    continue;
                }
                let has_x = m.x_axis.as_ref().is_some_and(|a| a.address > 0);
                let has_y = m.y_axis.as_ref().is_some_and(|a| a.address > 0);
                kept.push(RefItem {
                    address: m.address,
                    rows: m.rows,
                    cols: m.cols,
                    elem_bytes,
                    is_2d,
                    has_ref_axes: has_x && has_y,
                });
            }
            ParsedRefSummary {
                format_name: "XDF",
                raw_items,
                ram_excluded: 0,
                unmapped_excluded: 0,
                oob_excluded: oob,
                dedup_items: dedup_ref_items(kept),
                translator_note: "kivi_defs::xdf::parse_xdf_full (BASEOFFSET applied)".to_string(),
            }
        }
        RefFormat::A2l => {
            let doc = parse_a2l(&text)
                .unwrap_or_else(|e| panic!("FATAL: parse A2L '{}': {e}", full.display()));
            let tr = doc
                .build_translator_for_rom(rom_len)
                .unwrap_or_else(|| panic!("FATAL: no A2L translator for '{}'", full.display()));
            let seg_name = tr
                .segments
                .first()
                .map(|s| format!("{} (0x{:08X}..=0x{:08X})", s.name, s.cpu_start_address, s.cpu_end_address))
                .unwrap_or_default();

            let mut raw_items = 0usize;
            let mut unmapped = 0usize;
            let mut oob = 0usize;
            let mut kept = Vec::new();
            for c in &doc.characteristics {
                if !matches!(c.kind, A2lCharacteristicType::Map | A2lCharacteristicType::Curve) {
                    continue;
                }
                raw_items += 1;
                let Some((rows, cols)) = c.dimensions() else {
                    continue;
                };
                let is_2d = rows > 1 && cols > 1;
                let is_1d = (rows == 1 || cols == 1) && rows.max(cols) >= 4;
                if !is_2d && !is_1d {
                    continue;
                }
                let elem_bytes = doc
                    .record_layout_for(&c.record_layout)
                    .and_then(|rl| rl.fnc_element_bytes())
                    .unwrap_or(2);
                let raw_off = match cpu_to_raw(&tr, c.address) {
                    Ok(t) if !t.is_identity_fallback => t.value as usize,
                    _ => {
                        unmapped += 1;
                        continue;
                    }
                };
                let span = rows.saturating_mul(cols).saturating_mul(elem_bytes);
                if span == 0 || raw_off.saturating_add(span) > rom_len {
                    oob += 1;
                    continue;
                }
                let ref_axes_cnt = c.axes.iter().filter(|a| a.axis_pts_ref.is_some()).count();
                kept.push(RefItem {
                    address: raw_off,
                    rows,
                    cols,
                    elem_bytes,
                    is_2d,
                    has_ref_axes: is_2d && ref_axes_cnt >= 2,
                });
            }
            ParsedRefSummary {
                format_name: "A2L",
                raw_items,
                ram_excluded: 0,
                unmapped_excluded: unmapped,
                oob_excluded: oob,
                dedup_items: dedup_ref_items(kept),
                translator_note: seg_name,
            }
        }
        RefFormat::DamosMe7 => {
            let doc = parse_damos(&text)
                .unwrap_or_else(|e| panic!("FATAL: parse DAMOS '{}': {e}", full.display()));
            let raw_items = doc.maps.len() + doc.spz_objects.len();
            let mut ram_ex = 0usize;
            let mut unmapped_ex = 0usize;
            let mut oob = 0usize;
            let mut kept = Vec::new();
            for m in doc.maps {
                match translate_me7(m.address) {
                    Me7Translation::InternalRam => {
                        ram_ex += 1;
                    }
                    Me7Translation::Unmapped => {
                        unmapped_ex += 1;
                    }
                    Me7Translation::FileOffset(off) => {
                        let addr = off as usize;
                        let is_2d = m.rows > 1 && m.cols > 1;
                        let is_1d = (m.rows == 1 || m.cols == 1) && m.rows.max(m.cols) >= 4;
                        if !is_2d && !is_1d {
                            continue;
                        }
                        let elem_bytes = if m.mask > 0 && m.mask <= 0xFF { 1 } else { 2 };
                        let span = m.rows.saturating_mul(m.cols).saturating_mul(elem_bytes);
                        if span == 0 || addr.saturating_add(span) > rom_len {
                            oob += 1;
                            continue;
                        }
                        kept.push(RefItem {
                            address: addr,
                            rows: m.rows,
                            cols: m.cols,
                            elem_bytes,
                            is_2d,
                            has_ref_axes: false,
                        });
                    }
                }
            }
            for obj in doc.spz_objects {
                let is_2d = obj.is_2d_map();
                let is_1d = obj.is_1d_curve();
                if !is_2d && !is_1d {
                    continue;
                }
                match translate_me7(obj.address) {
                    Me7Translation::InternalRam => {
                        ram_ex += 1;
                    }
                    Me7Translation::Unmapped => {
                        unmapped_ex += 1;
                    }
                    Me7Translation::FileOffset(off) => {
                        let addr = off as usize;
                        let rows = if is_2d { obj.rows } else { 1 };
                        let cols = obj.cols;
                        let elem_bytes = obj.elem_bytes.max(1);
                        let span = rows.saturating_mul(cols).saturating_mul(elem_bytes);
                        if span == 0 || addr.saturating_add(span) > rom_len {
                            oob += 1;
                            continue;
                        }
                        kept.push(RefItem {
                            address: addr,
                            rows,
                            cols,
                            elem_bytes,
                            is_2d,
                            has_ref_axes: is_2d && matches!(obj.spz_kind, 3 | 8),
                        });
                    }
                }
            }
            ParsedRefSummary {
                format_name: "DAMOS",
                raw_items,
                ram_excluded: ram_ex,
                unmapped_excluded: unmapped_ex,
                oob_excluded: oob,
                dedup_items: dedup_ref_items(kept),
                translator_note:
                    "translate_me7 ($800000-$8FFFFF -> file offset); /SPZ Types 2/3/4/5/7/8/13 flash parsed, /UMP RAM/SFR excluded"
                        .to_string(),
            }
        }
    }
}

