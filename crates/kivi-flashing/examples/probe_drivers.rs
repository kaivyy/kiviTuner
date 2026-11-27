//! Probe the J2534 drivers found on this machine.
//!
//! Run with: cargo run -p kivi-flashing --features j2534-ffi --example probe_drivers
//!
//! Loading a Windows PE on Linux is expected to fail; what this demonstrates is
//! that the loader resolves the fourteen entry points when the platform allows
//! it, and fails cleanly with a named error when it does not.

fn main() {
    let cli_dirs: Vec<std::path::PathBuf> =
        std::env::args().skip(1).map(std::path::PathBuf::from).collect();
    let dirs: Vec<std::path::PathBuf> = if !cli_dirs.is_empty() {
        cli_dirs
    } else if let Ok(env_dirs) = std::env::var("J2534_DRIVER_DIRS") {
        std::env::split_paths(&env_dirs).collect()
    } else {
        vec![".".into()]
    };
    let refs: Vec<&std::path::Path> = dirs.iter().map(|p| p.as_path()).collect();
    let reg = kivi_flashing::j2534_loader::DriverRegistry::scan_directories(&refs);

    println!("discovered {} candidate driver(s):", reg.len());
    for c in reg.ranked() {
        println!("  score={:3} {:<28} {}", c.score, c.name, c.path);

        #[cfg(feature = "j2534-ffi")]
        match kivi_flashing::j2534_ffi::J2534EntryPoints::load(&c.path) {
            Ok(_) => println!("       -> loaded, all 14 PassThru entry points resolved"),
            Err(e) => println!("       -> {e}"),
        }
    }

    #[cfg(not(feature = "j2534-ffi"))]
    println!("\n(rebuild with --features j2534-ffi to attempt loading)");
}
