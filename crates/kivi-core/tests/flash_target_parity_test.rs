//! Behavioral parity tests for `kivi_core::flash_target` against the reference
//! implementation in `ecu_engine_rust::flashing::bench_protocol`
//! (`FlashSiliconClassifier`).
//!
//! Expected values were produced by executing the **unmodified** extracted
//! source through a disposable oracle harness.
//!
//! Note: the source table for `Am29F400BT` declares `total_size_bytes = 524288`
//! but its block lengths sum to `327680` (a 0x30000 gap between 0x40000 and
//! 0x70000). This is a **source data defect**, preserved verbatim and asserted
//! here so it cannot be silently "fixed" without a deliberate decision.

use kivi_core::flash_target::{
    describe_flash_target, is_block_write_safe, FlashBlockType, FlashTargetError,
};

#[test]
fn parity_identify_intel_28f200bx_both_ids() {
    for id in [0x00892274u32, 0x00892275] {
        let info = describe_flash_target(id).expect("identify");
        assert_eq!(info.model_name, "Intel 28F200BX 256KiB");
        assert_eq!(info.total_size_bytes, 262_144);
        assert_eq!(info.blocks.len(), 5);
        assert!(!is_block_write_safe(&info.blocks[0]));
        assert!(info.blocks[1..].iter().all(is_block_write_safe));
    }
}

#[test]
fn parity_identify_intel_28f400b() {
    let info = describe_flash_target(0x00894471).expect("identify");
    assert_eq!(info.model_name, "Intel 28F400B 512KiB");
    assert_eq!(info.total_size_bytes, 524_288);
    assert_eq!(info.blocks.len(), 7);
    assert_eq!(info.blocks[0].block_type, FlashBlockType::Boot);
    assert!(!is_block_write_safe(&info.blocks[0]));
}

#[test]
fn parity_identify_amd_am29f400bt_preserves_source_gap() {
    // Source declares 512 KiB but blocks sum to 327680 bytes.
    let info = describe_flash_target(0x00012258).expect("identify");
    assert_eq!(info.model_name, "AMD Am29F400BT 512KiB Top-Boot");
    assert_eq!(info.total_size_bytes, 524_288);

    let sum: u64 = info.blocks.iter().map(|b| b.length as u64).sum();
    assert_eq!(sum, 327_680, "source block table has a 0x30000 gap");

    // Top-boot layout: boot block is LAST, and only it is write-protected.
    let last = info.blocks.last().expect("blocks");
    assert_eq!(last.block_type, FlashBlockType::Boot);
    assert!(!is_block_write_safe(last));
    assert!(info.blocks[..info.blocks.len() - 1]
        .iter()
        .all(is_block_write_safe));
}

#[test]
fn parity_identify_amd_am29f800bb() {
    let info = describe_flash_target(0x000122D7).expect("identify");
    assert_eq!(info.model_name, "AMD Am29F800BB 1024KiB Bottom-Boot");
    assert_eq!(info.total_size_bytes, 1_048_576);
    assert_eq!(info.blocks.len(), 7);
    assert_eq!(info.blocks[0].block_type, FlashBlockType::Boot);
}

#[test]
fn parity_identify_unknown_ids_are_rejected() {
    for id in [
        0x0000_0000u32,
        0xFFFF_FFFF,
        0x0089_4470,
        0x0001_2257,
        0x1234_5678,
    ] {
        assert_eq!(
            describe_flash_target(id),
            Err(FlashTargetError::UnknownChipId),
            "id 0x{:08X} must be rejected",
            id
        );
    }
}

#[test]
fn parity_boot_block_is_never_write_safe() {
    for id in [0x00892274u32, 0x00894471, 0x00012258, 0x000122D7] {
        let info = describe_flash_target(id).expect("identify");
        let boot_blocks: Vec<_> = info
            .blocks
            .iter()
            .filter(|b| b.block_type == FlashBlockType::Boot)
            .collect();
        assert!(!boot_blocks.is_empty(), "id 0x{:08X} has no boot block", id);
        for b in boot_blocks {
            assert!(!is_block_write_safe(b));
        }
    }
}

#[test]
fn describe_is_deterministic() {
    let a = describe_flash_target(0x00894471).expect("identify");
    let b = describe_flash_target(0x00894471).expect("identify");
    assert_eq!(a, b);
}
