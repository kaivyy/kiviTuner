//! Cross-platform PassThru adapter discovery.
//!
//! ## The two real mechanisms
//!
//! Software that finds a J2534 interface does one of two things, and kiviTuner
//! previously did neither:
//!
//! | Platform | Where a driver registers itself |
//! |---|---|
//! | Windows | `HKLM\SOFTWARE\PassThruSupport.04.04\<Name>` — `FunctionLibrary`, `CAN`, `ISO15765`, `ISO14230` |
//! | Linux / macOS | `~/.passthru/*.json` — the de-facto convention used by `libj2534_openport2`, Macchina-J2534 and OpenVehicleDiag |
//!
//! A directory scan (which kiviTuner already had) only works if the user knows
//! the path. These two mechanisms are how a driver that was installed *properly*
//! becomes visible.
//!
//! ## What is implemented here
//!
//! The JSON side in full — it is portable and fully testable. The Windows
//! registry side needs Windows APIs, so it is described in
//! [`registry_key_paths`] but not read here; a Windows build supplies its own
//! reader rather than this crate pulling in a registry dependency it cannot
//! exercise.
//!
//! ## Capability flags matter
//!
//! `ISO15765` is what a CAN ROM dump needs. A K-Line-only interface reporting
//! `ISO9141: true, ISO15765: false` must not be offered for a CAN ECU, so the
//! flags are carried through rather than collapsed into "supported".

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A PassThru device discovered from a descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassthruDevice {
    pub name: String,
    pub vendor: String,
    /// Path to the `.dll` / `.so` / `.dylib` the descriptor names.
    pub library_path: PathBuf,
    pub supports_can: bool,
    pub supports_iso15765: bool,
    pub supports_iso9141: bool,
    pub supports_iso14230: bool,
}

/// The on-disk JSON shape.
///
/// Field names are upper-case because that is what the convention uses.
#[derive(Debug, Deserialize)]
struct RawDescriptor {
    #[serde(rename = "NAME")]
    name: Option<String>,
    #[serde(rename = "VENDOR")]
    vendor: Option<String>,
    #[serde(rename = "FUNCTION_LIB")]
    function_lib: Option<String>,
    #[serde(rename = "CAN")]
    can: Option<bool>,
    #[serde(rename = "ISO15765")]
    iso15765: Option<bool>,
    #[serde(rename = "ISO9141")]
    iso9141: Option<bool>,
    #[serde(rename = "ISO14230")]
    iso14230: Option<bool>,
}

/// Parse one `*.json` descriptor.
///
/// A descriptor with no `NAME` or no `FUNCTION_LIB` is rejected: an unnamed
/// device cannot be shown in a picker, and a device with no library cannot be
/// opened. Capability flags default to `false`, which is the safe reading.
pub fn parse_passthru_json(text: &str) -> Result<PassthruDevice, String> {
    let raw: RawDescriptor =
        serde_json::from_str(text).map_err(|e| format!("invalid descriptor JSON: {e}"))?;

    let name = raw
        .name
        .filter(|n| !n.trim().is_empty())
        .ok_or("descriptor has no NAME")?;
    let function_lib = raw
        .function_lib
        .filter(|p| !p.trim().is_empty())
        .ok_or("descriptor has no FUNCTION_LIB")?;

    Ok(PassthruDevice {
        name,
        vendor: raw.vendor.unwrap_or_default(),
        library_path: expand_home(&function_lib),
        supports_can: raw.can.unwrap_or(false),
        supports_iso15765: raw.iso15765.unwrap_or(false),
        supports_iso9141: raw.iso9141.unwrap_or(false),
        supports_iso14230: raw.iso14230.unwrap_or(false),
    })
}

/// Expands `%VAR%` Windows environment variables inside a path using a caller-supplied lookup.
pub fn expand_windows_env_vars_with<F>(raw: &str, lookup: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        if let Some(end) = after_start.find('%') {
            let var_name = &after_start[..end];
            if !var_name.is_empty() {
                if let Some(val) = lookup(var_name) {
                    out.push_str(&val);
                    rest = &after_start[end + 1..];
                    continue;
                }
            }
            out.push('%');
            out.push_str(var_name);
            out.push('%');
            rest = &after_start[end + 1..];
        } else {
            out.push('%');
            out.push_str(after_start);
            return out;
        }
    }
    out.push_str(rest);
    out
}

/// Expands `%VAR%` tokens using the host process environment (case-insensitive key match).
pub fn expand_windows_env_vars(raw: &str) -> String {
    expand_windows_env_vars_with(raw, |key| {
        std::env::var(key).ok().or_else(|| {
            std::env::vars()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v)
        })
    })
}

/// Expand a leading `~` to the user's home directory (`HOME` or `USERPROFILE` on Windows).
fn expand_home(path: &str) -> PathBuf {
    let expanded = expand_windows_env_vars(path);
    if let Some(rest) = expanded.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(expanded)
}

/// Scan one directory for `*.json` descriptors.
///
/// A broken descriptor is skipped rather than aborting the scan: one bad file
/// must not hide every good one.
pub fn scan_passthru_directory(dir: &Path) -> Vec<PassthruDevice> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_json = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("json"));
        if !is_json {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(device) = parse_passthru_json(&text) {
                out.push(device);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The directories a descriptor may live in, most specific first.
///
/// `~/.passthru` is the convention every Linux J2534 project uses;
/// `/usr/share/passthru` and `/etc/passthru` are the system-wide equivalents.
pub fn standard_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        paths.push(PathBuf::from(&home).join(".passthru"));
        paths.push(PathBuf::from(&home).join("Library/Application Support/PassThru"));
    }
    paths.push(PathBuf::from("/usr/share/passthru"));
    paths.push(PathBuf::from("/etc/passthru"));
    paths.push(PathBuf::from("/Library/Application Support/PassThru"));
    if let Ok(pd) = std::env::var("PROGRAMDATA") {
        paths.push(PathBuf::from(pd).join("PassThru"));
    }
    if let Ok(ad) = std::env::var("APPDATA") {
        paths.push(PathBuf::from(ad).join("PassThru"));
    }
    paths
}

/// Serial port descriptor discovered by OS enumeration (`serialport`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SerialPortDescriptor {
    pub port_name: String,
    pub description: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub is_openport2: bool,
    pub adapter_hint: String,
}

fn classify_usb_serial(vid: u16, pid: u16, desc_lower: &str) -> (bool, &'static str) {
    let is_tactrix_vid_pid = vid == 0x0403 && (0xCC48..=0xCC4D).contains(&pid);
    let is_tactrix_name = desc_lower.contains("openport")
        || desc_lower.contains("tactrix")
        || desc_lower.contains("op2.0")
        || desc_lower.contains("op20");
    if is_tactrix_vid_pid || is_tactrix_name {
        return (true, "openport2");
    }
    if desc_lower.contains("elm")
        || desc_lower.contains("obd")
        || desc_lower.contains("stn")
        || desc_lower.contains("vlinker")
    {
        return (false, "elm327_candidate");
    }
    if desc_lower.contains("bluetooth") || desc_lower.contains("rfcomm") {
        return (false, "bluetooth_spp");
    }
    let is_clone_usb_bridge = matches!(
        (vid, pid),
        (0x0483, 0x5740)
            | (0x0403, 0x6001)
            | (0x0403, 0x6015)
            | (0x1A86, 0x7523)
            | (0x1A86, 0x55D4)
            | (0x10C4, 0xEA60)
            | (0x067B, 0x2303)
    );
    if is_clone_usb_bridge {
        return (false, "tactrix_clone_candidate");
    }
    (false, "serial")
}

/// Enumerates real OS serial / USB-CDC ports (`ttyACM*`, `ttyUSB*`, `COM*`).
pub fn list_serial_ports() -> Vec<SerialPortDescriptor> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(ports.len());
    for p in ports {
        let (description, vid, pid, is_op2, adapter_hint) = match p.port_type {
            serialport::SerialPortType::UsbPort(u) => {
                let prod = u.product.unwrap_or_default();
                let mfg = u.manufacturer.unwrap_or_default();
                let desc = match (mfg.is_empty(), prod.is_empty()) {
                    (false, false) => format!("{mfg} {prod}"),
                    (false, true) => mfg,
                    (true, false) => prod.clone(),
                    (true, true) => format!("USB {:04X}:{:04X}", u.vid, u.pid),
                };
                let desc_lower = desc.to_ascii_lowercase();
                let (is_op2, hint) = classify_usb_serial(u.vid, u.pid, &desc_lower);
                (desc, Some(u.vid), Some(u.pid), is_op2, hint.to_string())
            }
            serialport::SerialPortType::PciPort => {
                ("PCI Serial".to_string(), None, None, false, "serial".to_string())
            }
            serialport::SerialPortType::BluetoothPort => (
                "Bluetooth SPP (ELM327/OBD candidate)".to_string(),
                None,
                None,
                false,
                "bluetooth_spp".to_string(),
            ),
            serialport::SerialPortType::Unknown => {
                let lower = p.port_name.to_ascii_lowercase();
                if lower.contains("rfcomm") {
                    (
                        "Bluetooth RFCOMM Serial".to_string(),
                        None,
                        None,
                        false,
                        "bluetooth_spp".to_string(),
                    )
                } else {
                    ("Serial Port".to_string(), None, None, false, "serial".to_string())
                }
            }
        };
        out.push(SerialPortDescriptor {
            port_name: p.port_name,
            description,
            vid,
            pid,
            is_openport2: is_op2,
            adapter_hint,
        });
    }
    out.sort_by(|a, b| a.port_name.cmp(&b.port_name));
    out
}

/// Lists active Linux SocketCAN interfaces (`can*`, `vcan*`, `slcan*`).
pub fn list_socketcan_interfaces() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        let Ok(entries) = std::fs::read_dir("/sys/class/net") else {
            return Vec::new();
        };
        let mut ifaces: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .filter(|name| {
                name.starts_with("can") || name.starts_with("vcan") || name.starts_with("slcan")
            })
            .collect();
        ifaces.sort();
        ifaces
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

fn scan_native_dir_for_ext(dir: &Path, ext: &str, vendor_label: &str, out: &mut Vec<PassthruDevice>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let fname = p
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let ext_ok = if ext == ".so" {
            fname.ends_with(".so") || fname.contains(".so.")
        } else {
            fname.ends_with(ext)
        };
        let name_ok = fname.contains("j2534")
            || fname.contains("passthru")
            || fname.contains("openport")
            || fname.contains("op20pt");
        if ext_ok && name_ok {
            out.push(PassthruDevice {
                name: p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("J2534")
                    .to_string(),
                vendor: vendor_label.to_string(),
                library_path: p,
                supports_can: true,
                supports_iso15765: true,
                supports_iso9141: true,
                supports_iso14230: true,
            });
        }
    }
}

/// Scans standard Linux library paths for native J2534 `.so` libraries.
pub fn scan_linux_passthru_libraries() -> Vec<PassthruDevice> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = vec![
        PathBuf::from("/usr/lib"),
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/usr/lib64"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu"),
        PathBuf::from("/usr/lib/aarch64-linux-gnu"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(&home).join(".passthru"));
        dirs.push(PathBuf::from(&home).join(".local/lib"));
    }
    for d in &dirs {
        scan_native_dir_for_ext(d, ".so", "Native Linux .so", &mut out);
    }
    out
}

/// Scans standard macOS library paths for native J2534 `.dylib` libraries.
pub fn scan_macos_passthru_libraries() -> Vec<PassthruDevice> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = vec![
        PathBuf::from("/opt/homebrew/lib"),
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/Library/Application Support/PassThru"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(&home).join("Library/Application Support/PassThru"));
        dirs.push(PathBuf::from(&home).join(".passthru"));
    }
    for d in &dirs {
        scan_native_dir_for_ext(d, ".dylib", "Native macOS .dylib", &mut out);
    }
    out
}

fn parse_reg_value_line(line: &str) -> Option<(&str, &str)> {
    for reg_type in ["REG_EXPAND_SZ", "REG_SZ", "REG_DWORD"] {
        if let Some(idx) = line.find(reg_type) {
            let key = line[..idx].trim();
            let val = line[idx + reg_type.len()..].trim();
            if !key.is_empty() {
                return Some((key, val));
            }
        }
    }
    None
}

/// Parses stdout from `reg query HKLM\...\PassThruSupport.04.04 /s` (preserves multi-space paths).
pub fn parse_windows_reg_query_output(text: &str) -> Vec<PassthruDevice> {
    let mut out = Vec::new();
    let mut cur_name: Option<String> = None;
    let mut cur_vendor = String::new();
    let mut cur_lib: Option<PathBuf> = None;
    let mut can = false;
    let mut iso15765 = false;
    let mut iso9141 = false;
    let mut iso14230 = false;
    let flush = |out: &mut Vec<PassthruDevice>,
                 name: &mut Option<String>,
                 vendor: &mut String,
                 lib: &mut Option<PathBuf>,
                 can: &mut bool,
                 iso15765: &mut bool,
                 iso9141: &mut bool,
                 iso14230: &mut bool| {
        if let (Some(n), Some(l)) = (name.take(), lib.take()) {
            out.push(PassthruDevice {
                name: n,
                vendor: std::mem::take(vendor),
                library_path: l,
                supports_can: *can,
                supports_iso15765: *iso15765,
                supports_iso9141: *iso9141,
                supports_iso14230: *iso14230,
            });
        }
        *can = false;
        *iso15765 = false;
        *iso9141 = false;
        *iso14230 = false;
    };
    let flag_enabled = |val: &str| val == "1" || val.ends_with("0x1") || val.ends_with("0x01");
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            flush(
                &mut out,
                &mut cur_name,
                &mut cur_vendor,
                &mut cur_lib,
                &mut can,
                &mut iso15765,
                &mut iso9141,
                &mut iso14230,
            );
            continue;
        }
        if line.starts_with("HKEY_") {
            flush(
                &mut out,
                &mut cur_name,
                &mut cur_vendor,
                &mut cur_lib,
                &mut can,
                &mut iso15765,
                &mut iso9141,
                &mut iso14230,
            );
            cur_name = line.rsplit('\\').next().map(|s| s.to_string());
            continue;
        }
        if let Some((key, val)) = parse_reg_value_line(line) {
            match key {
                "Name" => cur_name = Some(val.to_string()),
                "Vendor" => cur_vendor = val.to_string(),
                "FunctionLibrary" => {
                    let trimmed = val.trim_matches('"').trim();
                    if !trimmed.is_empty() {
                        cur_lib = Some(PathBuf::from(expand_windows_env_vars(trimmed)));
                    }
                }
                "CAN" => can = flag_enabled(val),
                "ISO15765" => iso15765 = flag_enabled(val),
                "ISO9141" => iso9141 = flag_enabled(val),
                "ISO14230" => iso14230 = flag_enabled(val),
                _ => {}
            }
        }
    }
    flush(
        &mut out,
        &mut cur_name,
        &mut cur_vendor,
        &mut cur_lib,
        &mut can,
        &mut iso15765,
        &mut iso9141,
        &mut iso14230,
    );
    out
}

/// Queries both 64-bit and 32-bit (WOW64) views of `HKLM\SOFTWARE\PassThruSupport.04.04` natively
/// via the Win32 Registry API (zero subprocess console flashes).
#[cfg(target_os = "windows")]
pub fn scan_windows_registry_passthru() -> Vec<PassthruDevice> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    use winreg::RegKey;

    let mut out = Vec::new();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let subkey_path = r"SOFTWARE\PassThruSupport.04.04";

    for flags in [KEY_READ | KEY_WOW64_64KEY, KEY_READ | KEY_WOW64_32KEY, KEY_READ] {
        let Ok(root) = hklm.open_subkey_with_flags(subkey_path, flags) else {
            continue;
        };
        for dev_key_name in root.enum_keys().flatten() {
            let Ok(dev_key) = root.open_subkey_with_flags(&dev_key_name, flags) else {
                continue;
            };
            let Ok(raw_lib) = dev_key.get_value::<String, _>("FunctionLibrary") else {
                continue;
            };
            let trimmed = raw_lib.trim_matches('"').trim();
            if trimmed.is_empty() {
                continue;
            }
            let lib_path = PathBuf::from(expand_windows_env_vars(trimmed));
            let name = dev_key
                .get_value::<String, _>("Name")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| dev_key_name.clone());
            let vendor = dev_key.get_value::<String, _>("Vendor").unwrap_or_default();
            let flag = |k: &str| dev_key.get_value::<u32, _>(k).is_ok_and(|v| v != 0);
            out.push(PassthruDevice {
                name,
                vendor,
                library_path: lib_path,
                supports_can: flag("CAN"),
                supports_iso15765: flag("ISO15765"),
                supports_iso9141: flag("ISO9141"),
                supports_iso14230: flag("ISO14230"),
            });
        }
    }
    out
}

fn normalized_dedup_key(dev: &PassthruDevice) -> (String, String) {
    let p = dev.library_path.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        (dev.name.to_ascii_lowercase(), p.replace('/', "\\").to_ascii_lowercase())
    }
    #[cfg(not(windows))]
    {
        (dev.name.clone(), p)
    }
}

/// Scan every standard path and merge the results.
pub fn scan_standard_paths() -> Vec<PassthruDevice> {
    let mut all = Vec::new();
    for dir in standard_search_paths() {
        all.extend(scan_passthru_directory(&dir));
    }
    #[cfg(target_os = "linux")]
    {
        all.extend(scan_linux_passthru_libraries());
    }
    #[cfg(target_os = "macos")]
    {
        all.extend(scan_macos_passthru_libraries());
    }
    #[cfg(target_os = "windows")]
    {
        all.extend(scan_windows_registry_passthru());
    }
    all.sort_by_key(normalized_dedup_key);
    all.dedup_by(|a, b| normalized_dedup_key(a) == normalized_dedup_key(b));
    all
}

/// The Windows registry key paths a J2534 driver registers under.
///
/// Windows builds query these keys natively via `winreg` (both 64-bit and
/// WOW64 32-bit views); non-Windows hosts expose them as reference metadata.
pub fn registry_key_paths() -> [&'static str; 2] {
    [
        r"SOFTWARE\PassThruSupport.04.04",
        r"SOFTWARE\WOW6432Node\PassThruSupport.04.04",
    ]
}
