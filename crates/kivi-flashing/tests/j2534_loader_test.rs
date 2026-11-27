//! Tests for J2534 driver discovery and loading.
//!
//! ## What can be tested without a Windows machine
//!
//! Discovery is pure filesystem work: given a set of directories, find files
//! whose names look like J2534 drivers. That is fully testable here.
//!
//! Loading is not: it needs `LoadLibrary`, a real vendor DLL, and Windows. The
//! loader is therefore behind the `j2534-ffi` feature, off by default, and these
//! tests cover only the discovery half plus the interface description.
//!
//! ## Why the name heuristics matter
//!
//! A wrong match means kiviTuner tries to load `something.dll` that is not a
//! J2534 driver, gets a confusing error, and the user concludes the tool is
//! broken. The heuristics below are deliberately conservative.

use kivi_flashing::j2534_loader::{
    candidate_score, has_library_extension, looks_like_j2534_driver, platform_library_extension,
    DriverCandidate, DriverRegistry, KNOWN_DRIVER_NAMES,
};

// ---------------------------------------------------------------------------
// Name recognition
// ---------------------------------------------------------------------------

#[test]
fn a_dll_named_op20_is_recognised() {
    // Tactrix OpenPort 2.0 32-bit J2534 PassThru DLL.
    assert!(looks_like_j2534_driver("op20pt32.dll"));
}

#[test]
fn a_dll_named_smj2534_is_recognised() {
    // Scanmatik.
    assert!(looks_like_j2534_driver("smj2534.dll"));
}

#[test]
fn a_dll_with_j2534_in_the_name_is_recognised() {
    assert!(looks_like_j2534_driver("MyVendorJ2534.dll"));
    assert!(looks_like_j2534_driver("j2534_v0404.dll"));
}

#[test]
fn a_non_driver_dll_is_rejected() {
    assert!(!looks_like_j2534_driver("kernel32.dll"));
    assert!(!looks_like_j2534_driver("user32.dll"));
    assert!(!looks_like_j2534_driver("op20pt32.lib"));
    assert!(!looks_like_j2534_driver("notes.txt"));
}

#[test]
fn matching_is_case_insensitive() {
    assert!(looks_like_j2534_driver("OP20PT32.DLL"));
    assert!(looks_like_j2534_driver("SMJ2534.Dll"));
}

#[test]
fn the_known_name_table_is_not_empty_and_contains_the_vendors_we_found() {
    assert!(!KNOWN_DRIVER_NAMES.is_empty());
    let joined = KNOWN_DRIVER_NAMES.join(" ").to_lowercase();
    for vendor in ["op20", "smj2534"] {
        assert!(joined.contains(vendor), "{vendor} must be in the table");
    }
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

#[test]
fn an_exact_known_name_scores_highest() {
    let exact = candidate_score("smj2534.dll");
    let partial = candidate_score("something_with_j2534_in_it.dll");
    assert!(
        exact > partial,
        "an exact vendor name ({exact}) must outrank a substring match ({partial})"
    );
}

#[test]
fn an_unrelated_name_scores_zero() {
    assert_eq!(candidate_score("totally_unrelated.dll"), 0);
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[test]
fn an_empty_registry_reports_nothing_found() {
    let reg = DriverRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
}

#[test]
fn adding_a_candidate_makes_it_discoverable() {
    let mut reg = DriverRegistry::new();
    reg.add(DriverCandidate {
        path: "/opt/j2534/smj2534.dll".to_string(),
        name: "smj2534.dll".to_string(),
        score: candidate_score("smj2534.dll"),
    });
    assert_eq!(reg.len(), 1);
    assert!(!reg.is_empty());
}

#[test]
fn candidates_are_ordered_by_score_descending() {
    let mut reg = DriverRegistry::new();
    reg.add(DriverCandidate {
        path: "/a/weak.dll".to_string(),
        name: "weak.dll".to_string(),
        score: 10,
    });
    reg.add(DriverCandidate {
        path: "/b/strong.dll".to_string(),
        name: "strong.dll".to_string(),
        score: 90,
    });
    reg.add(DriverCandidate {
        path: "/c/mid.dll".to_string(),
        name: "mid.dll".to_string(),
        score: 50,
    });

    let names: Vec<&str> = reg.ranked().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["strong.dll", "mid.dll", "weak.dll"]);
}

#[test]
fn the_best_candidate_is_the_top_of_the_ranking() {
    let mut reg = DriverRegistry::new();
    reg.add(DriverCandidate {
        path: "/a/weak.dll".to_string(),
        name: "weak.dll".to_string(),
        score: 10,
    });
    reg.add(DriverCandidate {
        path: "/b/strong.dll".to_string(),
        name: "strong.dll".to_string(),
        score: 90,
    });
    assert_eq!(reg.best().map(|c| c.name.as_str()), Some("strong.dll"));
}

#[test]
fn an_empty_registry_has_no_best_candidate() {
    assert_eq!(DriverRegistry::new().best(), None);
}

#[test]
fn adding_the_same_path_twice_does_not_duplicate_it() {
    let mut reg = DriverRegistry::new();
    let c = DriverCandidate {
        path: "/opt/x.dll".to_string(),
        name: "x.dll".to_string(),
        score: 5,
    };
    reg.add(c.clone());
    reg.add(c);
    assert_eq!(reg.len(), 1, "the same path must not appear twice");
}

// ---------------------------------------------------------------------------
// Scanning a directory
// ---------------------------------------------------------------------------

#[test]
fn scanning_a_directory_finds_only_driver_like_files() {
    let dir = std::env::temp_dir().join(format!("kivi_j2534_scan_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    std::fs::write(dir.join("smj2534.dll"), b"not really a dll").unwrap();
    std::fs::write(dir.join("op20pt32.dll"), b"not really a dll").unwrap();
    std::fs::write(dir.join("readme.txt"), b"notes").unwrap();
    std::fs::write(dir.join("random.dll"), b"nope").unwrap();

    let reg = DriverRegistry::scan_directory(&dir);
    let names: Vec<&str> = reg.ranked().iter().map(|c| c.name.as_str()).collect();

    assert_eq!(names.len(), 2, "only the two driver-like names: {names:?}");
    assert!(names.contains(&"smj2534.dll"));
    assert!(names.contains(&"op20pt32.dll"));
    assert!(!names.contains(&"random.dll"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scanning_a_missing_directory_yields_an_empty_registry() {
    // Must not panic: this runs on startup with user-supplied paths.
    let reg = DriverRegistry::scan_directory(std::path::Path::new(
        "/definitely/does/not/exist/kivi_j2534",
    ));
    assert!(reg.is_empty());
}

#[test]
fn scanning_is_case_insensitive_on_the_extension() {
    let dir = std::env::temp_dir().join(format!("kivi_j2534_case_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SMJ2534.DLL"), b"x").unwrap();

    let reg = DriverRegistry::scan_directory(&dir);
    assert_eq!(reg.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Cross-platform discovery
// ---------------------------------------------------------------------------

#[test]
fn a_windows_dll_is_discovered_even_on_a_non_windows_host() {
    // A driver sitting on a macOS or Linux disk must still be *reported*, so the
    // UI can say "found, but this machine cannot load it". Hiding it would
    // leave the user wondering where their driver went.
    assert!(looks_like_j2534_driver("smj2534.dll"));
}

#[test]
fn a_macos_dylib_is_recognised() {
    assert!(has_library_extension("libj2534.dylib"));
    assert!(looks_like_j2534_driver("libj2534.dylib"));
}

#[test]
fn a_linux_so_is_recognised() {
    assert!(has_library_extension("libj2534.so"));
    assert!(looks_like_j2534_driver("libj2534.so"));
}

#[test]
fn a_non_library_extension_is_still_rejected() {
    assert!(!has_library_extension("readme.txt"));
    assert!(!has_library_extension("driver.lib"));
    assert!(!looks_like_j2534_driver("j2534_readme.txt"));
}

#[test]
fn the_platform_extension_is_one_of_the_three() {
    let ext = platform_library_extension();
    assert!(
        [".dll", ".dylib", ".so"].contains(&ext),
        "unexpected platform extension: {ext}"
    );
}

#[test]
fn scanning_finds_drivers_of_any_library_extension() {
    let dir = std::env::temp_dir().join(format!("kivi_j2534_multi_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    std::fs::write(dir.join("smj2534.dll"), b"x").unwrap();
    std::fs::write(dir.join("libj2534.dylib"), b"x").unwrap();
    std::fs::write(dir.join("libj2534.so"), b"x").unwrap();
    std::fs::write(dir.join("notes.txt"), b"x").unwrap();

    let reg = DriverRegistry::scan_directory(&dir);
    assert_eq!(
        reg.len(),
        3,
        "all three library flavours must be discovered, got {:?}",
        reg.ranked().iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_pe_elf_and_macho_architectures_and_compatibility() {
    use kivi_flashing::j2534_loader::{
        describe_incompatibility, inspect_library_binary, is_binary_compatible_with, LibraryArch,
        LibraryFormat,
    };

    assert!(looks_like_j2534_driver("op20pt64.dll"));
    assert!(looks_like_j2534_driver("mvci32.dll"));
    assert!(looks_like_j2534_driver("libj2534_openport2.so"));
    assert!(looks_like_j2534_driver("libj2534_openport2.dylib"));
    assert!(has_library_extension("libj2534.so.1"));

    // PE helper
    let make_pe = |machine: u16| {
        let mut b = vec![0u8; 0x88];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        b[0x40..0x44].copy_from_slice(b"PE\0\0");
        b[0x44..0x46].copy_from_slice(&machine.to_le_bytes());
        b
    };
    let pe_x86 = inspect_library_binary(&make_pe(0x014C));
    assert_eq!(pe_x86.format, LibraryFormat::Pe);
    assert_eq!(pe_x86.arch, LibraryArch::X86);
    assert!(is_binary_compatible_with(&pe_x86, "windows", "x86"));
    assert!(!is_binary_compatible_with(&pe_x86, "windows", "x86_64"));
    assert!(describe_incompatibility(&pe_x86, "windows", "x86_64")
        .unwrap()
        .contains("OpenPort 2.0"));

    let pe_x64 = inspect_library_binary(&make_pe(0x8664));
    assert_eq!(pe_x64.arch, LibraryArch::X86_64);
    assert!(is_binary_compatible_with(&pe_x64, "windows", "x86_64"));

    // ELF helper
    let make_elf = |machine: u16| {
        let mut b = vec![0u8; 32];
        b[0..4].copy_from_slice(b"\x7fELF");
        b[5] = 1; // little endian
        b[18..20].copy_from_slice(&machine.to_le_bytes());
        b
    };
    let elf_x64 = inspect_library_binary(&make_elf(62));
    assert_eq!(elf_x64.format, LibraryFormat::Elf);
    assert_eq!(elf_x64.arch, LibraryArch::X86_64);
    assert!(is_binary_compatible_with(&elf_x64, "linux", "x86_64"));
    assert!(!is_binary_compatible_with(&elf_x64, "windows", "x86_64"));

    let elf_arm64 = inspect_library_binary(&make_elf(183));
    assert_eq!(elf_arm64.arch, LibraryArch::Aarch64);
    assert!(is_binary_compatible_with(&elf_arm64, "linux", "aarch64"));

    // Thin Mach-O ARM64
    let mut macho_arm = vec![0u8; 16];
    macho_arm[0..4].copy_from_slice(&0xFEED_FACFu32.to_be_bytes());
    macho_arm[4..8].copy_from_slice(&0x0100_000Cu32.to_be_bytes());
    let m_arm = inspect_library_binary(&macho_arm);
    assert_eq!(m_arm.format, LibraryFormat::MachO);
    assert_eq!(m_arm.arch, LibraryArch::Aarch64);
    assert!(is_binary_compatible_with(&m_arm, "macos", "aarch64"));
    assert!(!is_binary_compatible_with(&m_arm, "macos", "x86_64"));

    // Universal Fat Mach-O (x64 + ARM64)
    let mut fat = vec![0u8; 8 + 40];
    fat[0..4].copy_from_slice(&0xCAFE_BABEu32.to_be_bytes());
    fat[4..8].copy_from_slice(&2u32.to_be_bytes());
    fat[8..12].copy_from_slice(&0x0100_0007u32.to_be_bytes());
    fat[28..32].copy_from_slice(&0x0100_000Cu32.to_be_bytes());
    let m_uni = inspect_library_binary(&fat);
    assert_eq!(m_uni.arch, LibraryArch::Universal);
    assert!(is_binary_compatible_with(&m_uni, "macos", "x86_64"));
    assert!(is_binary_compatible_with(&m_uni, "macos", "aarch64"));
}
