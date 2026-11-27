pub mod address_translation;
pub mod bin_export;
pub mod flash_target;
pub mod hash;
pub mod hex_export;
pub mod hex_ingest;
pub mod ingest;
pub mod memory_map;
pub mod sector;
pub mod types;
pub mod undo_history;
pub mod verify;

pub use flash_target::{
    describe_flash_target, is_block_write_safe, FlashBlockDescriptor, FlashBlockType,
    FlashTargetError, FlashTargetInfo,
};
pub use hex_ingest::{
    detect_format, ingest_firmware, ingest_hex_text, ingest_srec_text, FirmwareFormat,
    HexIngestError, HexIngestResult, MAX_FIRMWARE_SPAN,
};
pub use sector::{
    plan_differential_flash, total_bytes_to_flash, DifferentialSectorPlan, SectorPlanError,
};
pub use verify::{check_blank, verify_buffer, BlankCheckResult, VerifyDifference};
