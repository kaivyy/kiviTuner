//! Identity-code cross-tabulation for the 647-pair catalogue.
//!
//! Reuses `kivi_scanner::rom_identity::compare_identity` rather than a bespoke
//! matcher, so the classification is the project's own definition of an
//! identity match. Read-only: no scanner is run, no production code changes.
//!
//! Run: `cargo run --release --example identity_code_audit`

use std::collections::HashMap;

use kivi_scanner::rom_identity::{compare_identity, extract_definition_tokens, extract_rom_tokens, IdentityMatch};

const DB: &str = "/root/ecu_master_database";
const CATALOGUE: &str = "/root/ecu_master_database/DAFTAR_PASANGAN_BIN_XDF_TERVERIFIKASI.md";

struct Row {
    bin: String,
    def: String,
    folder: String,
}

fn load_catalogue() -> Vec<Row> {
    let text = std::fs::read_to_string(CATALOGUE).expect("catalogue");
    let mut out = Vec::new();
    for line in text.lines() {
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(|c| c.trim()).collect();
        if cells.len() < 6 {
            continue;
        }
        let bin = cells[3].trim_matches('`');
        let def = cells[4].trim_matches('`');
        let folder = cells[5].trim_matches('`');
        if !bin.to_ascii_lowercase().ends_with(".bin")
            && !bin.to_ascii_lowercase().ends_with(".ori")
            && !bin.to_ascii_lowercase().ends_with(".rom")
        {
            continue;
        }
        out.push(Row {
            bin: bin.to_string(),
            def: def.to_string(),
            folder: folder.to_string(),
        });
    }
    out
}

fn main() {
    let rows = load_catalogue();
    println!("catalogue rows: {}", rows.len());

    let mut cache: HashMap<String, Vec<u8>> = HashMap::new();
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let mut details = Vec::new();

    for r in &rows {
        let bin_path = format!("{DB}/{}/{r}", r.folder, r = r.bin);
        let def_path = format!("{DB}/{}/{}", r.folder, r.def);
        let Ok(rom) = std::fs::read(&bin_path) else {
            *counts.entry("MISSING_BIN").or_default() += 1;
            continue;
        };
        if !std::path::Path::new(&def_path).exists() {
            *counts.entry("MISSING_DEF").or_default() += 1;
            continue;
        }
        cache.insert(bin_path.clone(), rom.clone());

        let verdict = match compare_identity(&rom, &r.def, "") {
            IdentityMatch::Shared { token } => {
                *counts.entry("EXACT_CODE_MATCH").or_default() += 1;
                format!("shared:{token}")
            }
            IdentityMatch::Mismatched { .. } => {
                *counts.entry("CODE_MISMATCH").or_default() += 1;
                "mismatched".to_string()
            }
            IdentityMatch::Inconclusive => {
                *counts.entry("NO_CODE_EVIDENCE").or_default() += 1;
                "inconclusive".to_string()
            }
        };

        // How many tokens each side produced, so an "inconclusive" verdict can be
        // told apart from "no tokens at all".
        let rt = extract_rom_tokens(&rom).len();
        let dt = extract_definition_tokens(&r.def, "").len();

        details.push(format!(
            "{{\"bin\":\"{}\",\"def\":\"{}\",\"verdict\":\"{}\",\"rom_tokens\":{},\"def_tokens\":{}}}",
            r.bin.replace('\\', "/").replace('"', "'"),
            r.def.replace('\\', "/").replace('"', "'"),
            verdict,
            rt,
            dt
        ));
    }

    println!("\nclassification:");
    let mut keys: Vec<&&str> = counts.keys().collect();
    keys.sort();
    for k in keys {
        println!("  {:<18} {}", k, counts[k]);
    }

    let json = format!("{{\n\"catalogue_rows\":{},\n\"counts\":{},\n\"pairs\":[\n{}\n]\n}}",
        rows.len(),
        serde_json::to_string(&counts).unwrap_or_else(|_| "{}".into()),
        details.join(",\n"));
    let out = "/root/kiviTuner/docs/migration/identity_code_audit_2026-10-30.json";
    std::fs::write(out, json).expect("write");
    println!("\nwrote {out}");
}
