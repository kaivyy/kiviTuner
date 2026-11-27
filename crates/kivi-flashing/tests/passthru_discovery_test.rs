//! Tests for cross-platform PassThru adapter discovery.
//!
//! ## Why this exists
//!
//! kiviTuner could find a driver by scanning a directory, but not by the two
//! mechanisms real J2534 software uses:
//!
//! - **Windows**: the registry key
//!   `HKLM\SOFTWARE\PassThruSupport.04.04\<DeviceName>`, whose `FunctionLibrary`
//!   value names the DLL and whose `CAN`/`ISO15765`/`ISO14230` values declare
//!   capability.
//! - **Linux / macOS**: `~/.passthru/*.json`, the de-facto convention used by
//!   `libj2534_openport2`, Macchina-J2534 and OpenVehicleDiag.
//!
//! Without those, a user who installed a driver properly still saw an empty
//! list. The JSON side is fully testable here; the registry side needs Windows
//! and is gated.

use kivi_flashing::passthru_discovery::{
    parse_passthru_json, scan_passthru_directory, PassthruDevice,
};

const SAMPLE: &str = r#"{
  "NAME": "Tactrix OpenPort 2.0",
  "VENDOR": "Tactrix Inc.",
  "FUNCTION_LIB": "/home/tuner/.passthru/libj2534_openport2.so",
  "CAN": true,
  "ISO15765": true,
  "ISO9141": true,
  "ISO14230": true
}"#;

// ---------------------------------------------------------------------------
// JSON parsing
// ---------------------------------------------------------------------------

#[test]
fn a_well_formed_json_descriptor_parses() {
    let d = parse_passthru_json(SAMPLE).expect("parse");
    assert_eq!(d.name, "Tactrix OpenPort 2.0");
    assert_eq!(d.vendor, "Tactrix Inc.");
    assert!(d.supports_can);
    assert!(d.supports_iso15765);
    assert!(d.supports_iso14230);
}

#[test]
fn the_library_path_is_carried_through() {
    let d = parse_passthru_json(SAMPLE).expect("parse");
    assert!(d.library_path.ends_with("libj2534_openport2.so"));
}

#[test]
fn a_descriptor_without_a_name_is_rejected() {
    // An unnamed device cannot be shown in a picker, so it is not a device.
    let no_name = SAMPLE.replace("\"NAME\": \"Tactrix OpenPort 2.0\",", "");
    assert!(parse_passthru_json(&no_name).is_err());
}

#[test]
fn a_descriptor_without_a_library_path_is_rejected() {
    let no_lib = SAMPLE.replace(
        "\"FUNCTION_LIB\": \"/home/tuner/.passthru/libj2534_openport2.so\",",
        "",
    );
    assert!(parse_passthru_json(&no_lib).is_err());
}

#[test]
fn missing_capability_flags_default_to_false() {
    // A minimal descriptor is legal; absent flags mean "not supported", which is
    // the safe reading.
    let minimal = r#"{"NAME":"Minimal","FUNCTION_LIB":"/x/lib.so"}"#;
    let d = parse_passthru_json(minimal).expect("parse");
    assert!(!d.supports_can);
    assert!(!d.supports_iso15765);
}

#[test]
fn malformed_json_is_rejected_not_panicked() {
    assert!(parse_passthru_json("{not json").is_err());
    assert!(parse_passthru_json("").is_err());
}

// ---------------------------------------------------------------------------
// Directory scanning
// ---------------------------------------------------------------------------

#[test]
fn scanning_a_directory_finds_json_descriptors() {
    let dir = std::env::temp_dir().join(format!("kivi_passthru_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    std::fs::write(dir.join("openport2.json"), SAMPLE).unwrap();
    std::fs::write(dir.join("notes.txt"), "not a descriptor").unwrap();

    let found = scan_passthru_directory(&dir);
    assert_eq!(found.len(), 1, "only the .json file is a descriptor");
    assert_eq!(found[0].name, "Tactrix OpenPort 2.0");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scanning_a_missing_directory_yields_nothing() {
    let found = scan_passthru_directory(std::path::Path::new("/no/such/passthru/dir"));
    assert!(found.is_empty());
}

#[test]
fn a_broken_descriptor_is_skipped_not_fatal() {
    let dir = std::env::temp_dir().join(format!("kivi_passthru_bad_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    std::fs::write(dir.join("broken.json"), "{not json").unwrap();
    std::fs::write(dir.join("good.json"), SAMPLE).unwrap();

    let found = scan_passthru_directory(&dir);
    assert_eq!(found.len(), 1, "the good descriptor must still be found");
    assert_eq!(found[0].name, "Tactrix OpenPort 2.0");

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Capability reporting
// ---------------------------------------------------------------------------

#[test]
fn a_device_reports_which_protocols_it_supports() {
    let d: PassthruDevice = parse_passthru_json(SAMPLE).expect("parse");
    assert!(d.supports_iso15765, "ISO15765 is what ROM dump needs");
}

#[test]
fn a_device_without_iso15765_cannot_dump_a_can_ecu() {
    let no_can = r#"{"NAME":"K-Line only","FUNCTION_LIB":"/x/lib.so","ISO9141":true}"#;
    let d = parse_passthru_json(no_can).expect("parse");
    assert!(!d.supports_iso15765);
}

#[test]
fn the_standard_search_paths_are_exposed() {
    // The list itself is the contract; the directories may not exist here.
    let paths = kivi_flashing::passthru_discovery::standard_search_paths();
    assert!(!paths.is_empty());
    assert!(
        paths.iter().any(|p| p.ends_with(".passthru")),
        "the ~/.passthru convention must be searched"
    );
}

#[test]
fn parses_windows_registry_passthru_entries_with_spaces_and_flags() {
    use kivi_flashing::passthru_discovery::parse_windows_reg_query_output;

    let sample = r#"
HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\PassThruSupport.04.04\OpenPort 2.0
    Name    REG_SZ    OpenPort 2.0 J2534 ISO/CAN/VPW/PWM
    Vendor    REG_SZ    Tactrix Inc.
    FunctionLibrary    REG_SZ    C:\Program Files (x86)\OpenECU\OpenPort 2.0\drivers\openport 2.0\op20pt32.dll
    CAN    REG_DWORD    0x1
    ISO15765    REG_DWORD    0x1
    ISO9141    REG_DWORD    0x1
    ISO14230    REG_DWORD    0x1

HKEY_LOCAL_MACHINE\SOFTWARE\PassThruSupport.04.04\XHorse - MVCI
    Name    REG_SZ    XHorse - MVCI
    Vendor    REG_SZ    XHorse
    FunctionLibrary    REG_SZ    "C:\Program Files\XHorse\MVCI32.dll"
    CAN    REG_DWORD    0x1
    ISO15765    REG_DWORD    0x1
    ISO9141    REG_DWORD    0x0
"#;

    let parsed = parse_windows_reg_query_output(sample);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].name, "OpenPort 2.0 J2534 ISO/CAN/VPW/PWM");
    assert!(parsed[0].library_path.to_string_lossy().contains("op20pt32.dll"));
    assert!(parsed[0].supports_can && parsed[0].supports_iso15765 && parsed[0].supports_iso14230);
    assert_eq!(parsed[1].name, "XHorse - MVCI");
    assert!(parsed[1].library_path.to_string_lossy().ends_with("MVCI32.dll"));
    assert!(!parsed[1].supports_iso9141);
}

#[test]
fn expands_windows_env_vars_and_preserves_multispace_registry_paths() {
    use kivi_flashing::passthru_discovery::{
        expand_windows_env_vars_with, parse_windows_reg_query_output,
    };

    let expanded = expand_windows_env_vars_with(
        r"%SystemRoot%\SysWOW64\op20pt32.dll",
        |k| match k.to_ascii_lowercase().as_str() {
            "systemroot" => Some(r"C:\Windows".to_string()),
            _ => None,
        },
    );
    assert_eq!(expanded, r"C:\Windows\SysWOW64\op20pt32.dll");

    // Verify unknown variables stay intact without hanging
    assert_eq!(
        expand_windows_env_vars_with("%UNKNOWN_VAR%\\drv.dll", |_| None),
        "%UNKNOWN_VAR%\\drv.dll"
    );

    let sample = concat!(
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\PassThruSupport.04.04\\My  Device\n",
        "    Name    REG_SZ    Vendor  Dual  Space\n",
        "    FunctionLibrary    REG_EXPAND_SZ    \"C:\\Vendor\\Two  Spaces\\mvci32.dll\"\n",
        "    CAN    REG_DWORD    0x1\n",
        "    ISO15765    REG_DWORD    0x1\n",
    );
    let parsed = parse_windows_reg_query_output(sample);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].name, "Vendor  Dual  Space");
    assert_eq!(
        parsed[0].library_path.to_string_lossy(),
        "C:\\Vendor\\Two  Spaces\\mvci32.dll"
    );
}
