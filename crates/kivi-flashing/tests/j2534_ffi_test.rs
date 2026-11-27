//! Tests for the real J2534 FFI binding.
//!
//! ## Gated on purpose
//!
//! Everything here needs `--features j2534-ffi`, which pulls in `libloading`
//! and the platform's dynamic loader. A default build compiles none of it, so
//! the offline suite stays platform-independent and `unsafe`-free.
//!
//! ## What can be proven without a vendor DLL
//!
//! The *symbol table* can: loading a library that is not a J2534 driver must
//! fail cleanly rather than crash, and the list of entry points the binding
//! expects must match the fourteen that `smj2534.dll` actually exports.
//!
//! Running the loaded functions is **not** tested. That needs a real interface,
//! a real bus and a real ECU, and claiming otherwise would be a lie.

#![cfg(feature = "j2534-ffi")]

use kivi_flashing::j2534_ffi::{J2534EntryPoints, J2534_REQUIRED_SYMBOLS};

#[test]
fn the_binding_expects_the_fourteen_standard_entry_points() {
    // Standard SAE J2534-1 v04.04 drivers export these 14 named PassThru functions.
    assert_eq!(J2534_REQUIRED_SYMBOLS.len(), 14);
}

#[test]
fn every_required_symbol_is_a_passthru_name() {
    for sym in J2534_REQUIRED_SYMBOLS {
        assert!(
            sym.starts_with("PassThru"),
            "{sym} is not a PassThru entry point"
        );
    }
}

#[test]
fn the_symbol_list_matches_the_scanmatik_exports() {
    // The exact set read from smj2534.dll's export table.
    let expected = [
        "PassThruClose",
        "PassThruConnect",
        "PassThruDisconnect",
        "PassThruGetLastError",
        "PassThruIoctl",
        "PassThruOpen",
        "PassThruReadMsgs",
        "PassThruReadVersion",
        "PassThruSetProgrammingVoltage",
        "PassThruStartMsgFilter",
        "PassThruStartPeriodicMsg",
        "PassThruStopMsgFilter",
        "PassThruStopPeriodicMsg",
        "PassThruWriteMsgs",
    ];
    let mut got: Vec<&str> = J2534_REQUIRED_SYMBOLS.to_vec();
    let mut want: Vec<&str> = expected.to_vec();
    got.sort_unstable();
    want.sort_unstable();
    assert_eq!(got, want);
}

#[test]
fn loading_a_missing_library_fails_cleanly() {
    let result = J2534EntryPoints::load("/definitely/not/a/real/j2534_driver.so");
    assert!(result.is_err(), "loading a missing library must not succeed");
}

#[test]
fn loading_a_non_library_file_fails_cleanly() {
    // A text file is not a shared object; the loader must refuse it rather
    // than crash.
    let dir = std::env::temp_dir().join(format!("kivi_ffi_bad_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("not_a_library.so");
    std::fs::write(&path, b"this is not an ELF or PE image").unwrap();

    let result = J2534EntryPoints::load(&path);
    assert!(result.is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_library_without_passthru_symbols_is_rejected() {
    // libc is definitely a real shared object and definitely not a J2534
    // driver, so symbol resolution must fail.
    let result = J2534EntryPoints::load("libc.so.6");
    match result {
        Ok(_) => panic!("libc must not resolve as a J2534 driver"),
        Err(e) => {
            // Either the library could not be opened or a symbol was missing;
            // both are acceptable, and the message must name the problem.
            let text = e.to_string();
            assert!(
                text.contains("symbol") || text.contains("load") || text.contains("open"),
                "unhelpful error: {text}"
            );
        }
    }
}
