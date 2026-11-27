//! J2534 driver discovery and (optionally) loading.
//!
//! ## Split by what needs a real machine
//!
//! Discovery is plain filesystem work and runs everywhere. Loading needs
//! `LoadLibrary`, a vendor DLL and Windows, so it lives behind the
//! `j2534-ffi` feature which is **off by default**. A default build therefore
//! has no `unsafe`, no dynamic linking and no platform dependency, which keeps
//! the offline test suite meaningful.
//!
//! ## Known vendor filenames
//!
//! `KNOWN_DRIVER_NAMES` prioritizes common SAE J2534 PassThru adapter DLLs
//! (`op20pt32.dll`/`op20pt64.dll` for Tactrix OpenPort 2.0, `smj2534*.dll`
//! for Scanmatik, `mvci*.dll` for Mini-VCI, and similar PassThru drivers).
//!
//! The list is a *hint for ranking*, never a whitelist: any file whose name
//! contains `j2534` is still a candidate, because vendors rename freely.

use std::path::Path;

/// Common SAE J2534 PassThru driver filenames used as a ranking hint.
pub const KNOWN_DRIVER_NAMES: &[&str] = &[
    "op20pt64.dll",
    "op20pt32.dll",
    "smj2534_64.dll",
    "smj2534x64.dll",
    "smj2534.dll",
    "smj2534_0202_usb.dll",
    "smj2534_0202_usb_sm3.dll",
    "smj2534_0404_usb_sm3.dll",
    "mvci64.dll",
    "mvci32.dll",
    "gd101j2534.dll",
    "godiag_j2534.dll",
    "j2534dotnet.dll",
    "j2534.dll",
    "passthru.dll",
    "libj2534_openport2.so",
    "libop20pt.so",
    "libop20pt64.so",
    "libsmj2534.so",
    "libj2534.so",
    "libpassthru.so",
    "libj2534_openport2.dylib",
    "libop20pt.dylib",
    "libj2534.dylib",
    "libpassthru.dylib",
];

/// Binary container format of a discovered shared library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryFormat {
    Pe,
    Elf,
    MachO,
    Unknown,
}

/// CPU architecture encoded inside a shared library header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryArch {
    X86,
    X86_64,
    Arm,
    Aarch64,
    Universal,
    Unknown,
}

/// Parsed format & architecture summary for a `.dll`, `.so`, or `.dylib`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryBinaryInfo {
    pub format: LibraryFormat,
    pub arch: LibraryArch,
    pub label: String,
}

fn mach_cpu_to_arch(cpu_type: u32) -> LibraryArch {
    match cpu_type {
        0x0100_0007 => LibraryArch::X86_64,
        0x0100_000C => LibraryArch::Aarch64,
        7 => LibraryArch::X86,
        12 => LibraryArch::Arm,
        _ => LibraryArch::Unknown,
    }
}

/// Inspects PE (`.dll`), ELF (`.so`), and Mach-O / Fat (`.dylib`) headers safely.
pub fn inspect_library_binary(bytes: &[u8]) -> LibraryBinaryInfo {
    if bytes.len() >= 0x40 && &bytes[0..2] == b"MZ" {
        let pe_off = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
        if let Some(end) = pe_off.checked_add(6) {
            if end <= bytes.len() && &bytes[pe_off..pe_off + 4] == b"PE\0\0" {
                let machine = u16::from_le_bytes([bytes[pe_off + 4], bytes[pe_off + 5]]);
                let (arch, label) = match machine {
                    0x014C => (LibraryArch::X86, "PE x86 (32-bit DLL)"),
                    0x8664 => (LibraryArch::X86_64, "PE x64 (64-bit DLL)"),
                    0xAA64 => (LibraryArch::Aarch64, "PE ARM64 DLL"),
                    _ => (LibraryArch::Unknown, "PE (unknown arch)"),
                };
                return LibraryBinaryInfo {
                    format: LibraryFormat::Pe,
                    arch,
                    label: label.to_string(),
                };
            }
        }
        return LibraryBinaryInfo {
            format: LibraryFormat::Pe,
            arch: LibraryArch::Unknown,
            label: "PE (unreadable header)".to_string(),
        };
    }

    if bytes.len() >= 20 && &bytes[0..4] == b"\x7fELF" {
        let is_be = bytes[5] == 2;
        let machine = if is_be {
            u16::from_be_bytes([bytes[18], bytes[19]])
        } else {
            u16::from_le_bytes([bytes[18], bytes[19]])
        };
        let (arch, label) = match machine {
            3 => (LibraryArch::X86, "ELF x86 (32-bit .so)"),
            62 => (LibraryArch::X86_64, "ELF x64 (64-bit .so)"),
            40 => (LibraryArch::Arm, "ELF ARM (32-bit .so)"),
            183 => (LibraryArch::Aarch64, "ELF ARM64 (.so)"),
            _ => (LibraryArch::Unknown, "ELF (unknown arch)"),
        };
        return LibraryBinaryInfo {
            format: LibraryFormat::Elf,
            arch,
            label: label.to_string(),
        };
    }

    if bytes.len() >= 8 {
        let magic_be = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic_be == 0xFEED_FACF || magic_be == 0xFEED_FACE {
            let cpu = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let arch = mach_cpu_to_arch(cpu);
            let label = match arch {
                LibraryArch::X86_64 => "Mach-O x64 (.dylib)",
                LibraryArch::Aarch64 => "Mach-O ARM64 (.dylib)",
                LibraryArch::X86 => "Mach-O x86 (32-bit .dylib)",
                LibraryArch::Arm => "Mach-O ARM (.dylib)",
                _ => "Mach-O (unknown arch)",
            };
            return LibraryBinaryInfo {
                format: LibraryFormat::MachO,
                arch,
                label: label.to_string(),
            };
        }
        if magic_be == 0xCFFA_EDFE || magic_be == 0xCEFA_EDFE {
            let cpu = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let arch = mach_cpu_to_arch(cpu);
            let label = match arch {
                LibraryArch::X86_64 => "Mach-O x64 (.dylib)",
                LibraryArch::Aarch64 => "Mach-O ARM64 (.dylib)",
                LibraryArch::X86 => "Mach-O x86 (32-bit .dylib)",
                LibraryArch::Arm => "Mach-O ARM (.dylib)",
                _ => "Mach-O (unknown arch)",
            };
            return LibraryBinaryInfo {
                format: LibraryFormat::MachO,
                arch,
                label: label.to_string(),
            };
        }
        if magic_be == 0xCAFE_BABE || magic_be == 0xCAFE_BABF {
            let entry_size = if magic_be == 0xCAFE_BABF { 32usize } else { 20usize };
            let nfat = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
            if nfat > 0 && nfat <= 16 && 8 + nfat * entry_size <= bytes.len() {
                let mut found: Vec<LibraryArch> = Vec::new();
                for i in 0..nfat {
                    let off = 8 + i * entry_size;
                    let cpu = u32::from_be_bytes([
                        bytes[off],
                        bytes[off + 1],
                        bytes[off + 2],
                        bytes[off + 3],
                    ]);
                    let arch = mach_cpu_to_arch(cpu);
                    if arch != LibraryArch::Unknown && !found.contains(&arch) {
                        found.push(arch);
                    }
                }
                if found.len() >= 2 {
                    return LibraryBinaryInfo {
                        format: LibraryFormat::MachO,
                        arch: LibraryArch::Universal,
                        label: "Mach-O Universal (.dylib)".to_string(),
                    };
                }
                if let Some(&single) = found.first() {
                    let label = match single {
                        LibraryArch::X86_64 => "Mach-O x64 (.dylib)",
                        LibraryArch::Aarch64 => "Mach-O ARM64 (.dylib)",
                        _ => "Mach-O (.dylib)",
                    };
                    return LibraryBinaryInfo {
                        format: LibraryFormat::MachO,
                        arch: single,
                        label: label.to_string(),
                    };
                }
            }
        }
    }

    LibraryBinaryInfo {
        format: LibraryFormat::Unknown,
        arch: LibraryArch::Unknown,
        label: "Unknown binary format".to_string(),
    }
}

/// Checks whether a parsed library matches the given OS & CPU architecture.
pub fn is_binary_compatible_with(
    info: &LibraryBinaryInfo,
    target_os: &str,
    target_arch: &str,
) -> bool {
    let arch_matches = match target_arch {
        "x86_64" => info.arch == LibraryArch::X86_64,
        "x86" => info.arch == LibraryArch::X86,
        "aarch64" => info.arch == LibraryArch::Aarch64,
        "arm" => info.arch == LibraryArch::Arm,
        _ => false,
    };
    match target_os {
        "windows" => info.format == LibraryFormat::Pe && arch_matches,
        "linux" => info.format == LibraryFormat::Elf && arch_matches,
        "macos" => {
            info.format == LibraryFormat::MachO
                && (info.arch == LibraryArch::Universal || arch_matches)
        }
        _ => false,
    }
}

/// Checks compatibility against the running host binary's OS & CPU architecture.
pub fn is_binary_compatible_with_host(info: &LibraryBinaryInfo) -> bool {
    is_binary_compatible_with(info, std::env::consts::OS, std::env::consts::ARCH)
}

/// Returns true if a 32-bit x86 Windows J2534 DLL can be hosted out-of-process
/// from a 64-bit Windows build using the companion 32-bit bridge executable.
pub fn can_load_via_32bit_bridge(
    info: &LibraryBinaryInfo,
    host_os: &str,
    host_arch: &str,
    bridge_available: bool,
) -> bool {
    bridge_available
        && host_os == "windows"
        && host_arch == "x86_64"
        && info.format == LibraryFormat::Pe
        && info.arch == LibraryArch::X86
}

/// Explains why a library cannot be loaded into the target process, if incompatible.
pub fn describe_incompatibility(
    info: &LibraryBinaryInfo,
    host_os: &str,
    host_arch: &str,
) -> Option<String> {
    describe_incompatibility_with_bridge(info, host_os, host_arch, false)
}

/// Explains why a library cannot be loaded, accounting for 32-bit bridge availability.
pub fn describe_incompatibility_with_bridge(
    info: &LibraryBinaryInfo,
    host_os: &str,
    host_arch: &str,
    bridge_available: bool,
) -> Option<String> {
    if is_binary_compatible_with(info, host_os, host_arch)
        || can_load_via_32bit_bridge(info, host_os, host_arch, bridge_available)
    {
        return None;
    }
    if info.format == LibraryFormat::Pe
        && info.arch == LibraryArch::X86
        && host_os == "windows"
        && host_arch == "x86_64"
    {
        return Some(
            "32-bit x86 DLL cannot load directly inside a 64-bit app and the 32-bit J2534 bridge helper was not found; use native OpenPort 2.0 USB-CDC or install the Win32 build."
                .to_string(),
        );
    }
    Some(format!(
        "{} is not loadable on {}/{}",
        info.label, host_os, host_arch
    ))
}

/// A discovered driver file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverCandidate {
    pub path: String,
    pub name: String,
    /// Higher is a better match. See [`candidate_score`].
    pub score: u32,
}

/// Score how strongly a filename looks like a J2534 driver.
///
/// Three tiers, highest first:
/// - exact match against [`KNOWN_DRIVER_NAMES`] → 100
/// - the name contains `j2534` → 60
/// - otherwise → 0
pub fn candidate_score(name: &str) -> u32 {
    let lower = name.to_ascii_lowercase();
    if KNOWN_DRIVER_NAMES.iter().any(|k| *k == lower) {
        return 100;
    }
    for hint in ["j2534", "passthru", "openport", "op20pt", "mvci"] {
        if lower.contains(hint) {
            return 60;
        }
    }
    0
}

/// The shared-library extension this platform can load.
///
/// J2534 drivers are Windows DLLs by definition, but a macOS or Linux build must
/// still be able to *scan* a directory (for display, and to tell the user
/// "this driver exists but this machine cannot load it"). Recognising the native
/// extension is what makes that message possible instead of silently showing
/// nothing.
pub fn platform_library_extension() -> &'static str {
    #[cfg(windows)]
    {
        ".dll"
    }
    #[cfg(target_os = "macos")]
    {
        ".dylib"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ".so"
    }
    #[cfg(not(any(windows, unix)))]
    {
        ".dll"
    }
}

/// Whether a filename has an extension a shared library can carry on any
/// supported platform.
///
/// Deliberately accepts all three, not just the host's: a Windows driver sitting
/// on a macOS disk should be *discovered and reported as unloadable*, not hidden.
/// Hiding it would leave the user wondering why their driver is not listed.
pub fn has_library_extension(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".dll")
        || lower.ends_with(".dylib")
        || lower.ends_with(".so")
        || lower.contains(".so.")
}

/// Whether a filename should be considered a driver at all.
///
/// Requires a shared-library extension and a non-zero score, so `readme.txt` and
/// `random.dll` are both excluded. Note that `random.dll` is excluded by its
/// *name*, not its extension — an arbitrary DLL is not a J2534 driver.
pub fn looks_like_j2534_driver(name: &str) -> bool {
    if !has_library_extension(name) {
        return false;
    }
    candidate_score(name) > 0
}

/// A set of discovered drivers, deduplicated by path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriverRegistry {
    candidates: Vec<DriverCandidate>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
        }
    }

    /// Add a candidate, ignoring a path already present (case-insensitive on Windows).
    pub fn add(&mut self, candidate: DriverCandidate) {
        #[cfg(windows)]
        let exists = self
            .candidates
            .iter()
            .any(|c| c.path.eq_ignore_ascii_case(&candidate.path));
        #[cfg(not(windows))]
        let exists = self.candidates.iter().any(|c| c.path == candidate.path);
        if exists {
            return;
        }
        self.candidates.push(candidate);
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// Candidates sorted by score, highest first, then by name for determinism.
    pub fn ranked(&self) -> Vec<&DriverCandidate> {
        let mut out: Vec<&DriverCandidate> = self.candidates.iter().collect();
        out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
        out
    }

    /// The highest-scoring candidate, if any.
    pub fn best(&self) -> Option<&DriverCandidate> {
        self.ranked().into_iter().next()
    }

    /// Scan one directory (non-recursive) for driver-like files.
    ///
    /// A missing or unreadable directory yields an empty registry rather than an
    /// error: this runs at startup against user-supplied paths, and failing
    /// startup because one search path does not exist would be hostile.
    pub fn scan_directory(dir: &Path) -> Self {
        let mut reg = Self::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return reg,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !looks_like_j2534_driver(&name) {
                continue;
            }
            reg.add(DriverCandidate {
                path: path.to_string_lossy().to_string(),
                score: candidate_score(&name),
                name,
            });
        }
        reg
    }

    /// Scan several directories and merge the results.
    pub fn scan_directories(dirs: &[&Path]) -> Self {
        let mut reg = Self::new();
        for dir in dirs {
            for c in Self::scan_directory(dir).candidates {
                reg.add(c);
            }
        }
        reg
    }

    /// The candidate list, unranked.
    pub fn candidates(&self) -> &[DriverCandidate] {
        &self.candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_64_detects_32bit_bridge_eligibility_accurately() {
        let pe32 = LibraryBinaryInfo {
            format: LibraryFormat::Pe,
            arch: LibraryArch::X86,
            label: "PE x86 (32-bit DLL)".to_string(),
        };
        assert!(can_load_via_32bit_bridge(&pe32, "windows", "x86_64", true));
        assert!(!can_load_via_32bit_bridge(&pe32, "windows", "x86_64", false));
        assert!(!can_load_via_32bit_bridge(&pe32, "windows", "x86", true));
        assert!(!can_load_via_32bit_bridge(&pe32, "linux", "x86_64", true));

        assert_eq!(
            describe_incompatibility_with_bridge(&pe32, "windows", "x86_64", true),
            None
        );
        let msg = describe_incompatibility_with_bridge(&pe32, "windows", "x86_64", false)
            .expect("missing bridge explains limitation");
        assert!(msg.contains("32-bit"));
        assert!(is_binary_compatible_with(&pe32, "windows", "x86"));
    }
}
