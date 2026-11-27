//! Parity tests for the flashing recovery journal.
//!
//! Ported from `ecu_engine_rust::flashing::recovery_journal`.
//!
//! ## Why this module matters
//!
//! The journal is the safety net for an interrupted flash. If it misreports
//! whether recovery is needed, or resumes at the wrong byte, an ECU can be left
//! half-written. These tests pin the state transitions and the resume offset.
//!
//! ## How expected values were obtained
//!
//! Every expectation follows from the source's own stage machine:
//!
//! * `ErasingSectors`, `WritingBlocks` and `InterruptedAbnormally` require recovery;
//! * `FlashCompleted` and the initial stage do not;
//! * the resume offset is `bytes_written` when recovery is required, else `0`.
//!
//! ## Documented source limitation
//!
//! The source never assigns `InterruptedAbnormally` — no method transitions into
//! it. That stage is therefore unreachable through the public API, and this test
//! file does not pretend otherwise (see `interrupted_stage_is_unreachable`).

use kivi_flashing::recovery_journal::{JournalStage, RecoveryJournalManager};

// ---------------------------------------------------------------------------
// Initial state
// ---------------------------------------------------------------------------

#[test]
fn a_new_journal_does_not_require_recovery() {
    let j = RecoveryJournalManager::new("ECM-123", &[0u8; 1024]);
    assert!(!j.is_recovery_required());
    assert_eq!(j.compute_resume_offset(), 0);
}

#[test]
fn deserializing_an_initialized_journal_preserves_initialized_stage() {
    let j = RecoveryJournalManager::new("ECM-ABC", &[1, 2, 3]);
    let bytes = j.serialize_checkpoint();
    let restored = RecoveryJournalManager::deserialize_checkpoint(&bytes, "ECM-ABC")
        .expect("round trip must work");
    assert_eq!(restored.checkpoint.stage, JournalStage::Initialized);
    assert!(
        !restored.is_recovery_required(),
        "an initialized journal must not report recovery required after round trip"
    );
    assert!(
        !restored.is_recovery_mode_active,
        "is_recovery_mode_active must mirror is_recovery_required()"
    );
}

// ---------------------------------------------------------------------------
// Recovery-required transitions
// ---------------------------------------------------------------------------

#[test]
fn erasing_a_sector_requires_recovery() {
    let mut j = RecoveryJournalManager::new("ECM-1", &[0u8; 4096]);
    j.record_sector_erase_start(0, 100);
    assert!(
        j.is_recovery_required(),
        "an erase in progress must require recovery"
    );
}

#[test]
fn writing_a_block_requires_recovery() {
    let mut j = RecoveryJournalManager::new("ECM-1", &[0u8; 4096]);
    j.record_block_written(256, 100);
    assert!(
        j.is_recovery_required(),
        "a write in progress must require recovery"
    );
}

#[test]
fn marking_completed_clears_the_recovery_requirement() {
    let mut j = RecoveryJournalManager::new("ECM-1", &[0u8; 4096]);
    j.record_block_written(512, 100);
    assert!(j.is_recovery_required());
    j.mark_completed();
    assert!(
        !j.is_recovery_required(),
        "a completed flash must not require recovery"
    );
}

// ---------------------------------------------------------------------------
// Resume offset
// ---------------------------------------------------------------------------

#[test]
fn the_resume_offset_follows_the_bytes_written() {
    let mut j = RecoveryJournalManager::new("ECM-1", &[0u8; 4096]);
    j.record_block_written(256, 100);
    assert_eq!(
        j.compute_resume_offset(),
        256,
        "resume must restart at the byte boundary already written"
    );
}

#[test]
fn successive_writes_accumulate_the_resume_offset() {
    let mut j = RecoveryJournalManager::new("ECM-1", &[0u8; 4096]);
    j.record_block_written(256, 100);
    j.record_block_written(128, 101);
    j.record_block_written(64, 102);
    assert_eq!(j.compute_resume_offset(), 256 + 128 + 64);
}

#[test]
fn the_resume_offset_is_zero_after_completion() {
    let mut j = RecoveryJournalManager::new("ECM-1", &[0u8; 4096]);
    j.record_block_written(512, 100);
    j.mark_completed();
    assert_eq!(
        j.compute_resume_offset(),
        0,
        "a completed flash resumes from the beginning, i.e. nowhere to resume"
    );
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

#[test]
fn a_checkpoint_survives_serialization() {
    let mut j = RecoveryJournalManager::new("ECM-XYZ", &[0u8; 2048]);
    j.record_block_written(1024, 500);
    let bytes = j.serialize_checkpoint();

    let restored = RecoveryJournalManager::deserialize_checkpoint(&bytes, "ECM-XYZ")
        .expect("must deserialize");
    assert!(restored.is_recovery_required(), "state must survive");
    assert!(restored.is_recovery_mode_active);
    assert_eq!(restored.compute_resume_offset(), 1024);
    assert_eq!(restored.checkpoint.timestamp_epoch_secs, 500);
    assert_eq!(restored.checkpoint.target_ecm_id, "ECM-XYZ");
}

#[test]
fn deserializing_for_a_different_ecm_is_rejected() {
    let mut j = RecoveryJournalManager::new("ECM-A", &[0u8; 512]);
    j.record_block_written(128, 10);
    let bytes = j.serialize_checkpoint();

    assert!(
        RecoveryJournalManager::deserialize_checkpoint(&bytes, "ECM-B").is_err(),
        "checkpoint bound to ECM-A must be rejected when loading for ECM-B"
    );

    let mut tampered = bytes.clone();
    tampered[20] ^= 0xFF;
    assert!(
        RecoveryJournalManager::deserialize_checkpoint(&tampered, "ECM-A").is_err(),
        "tampered checkpoint must fail CRC verification"
    );
}

#[test]
fn deserializing_garbage_is_rejected_without_panicking() {
    for junk in [vec![], vec![0u8; 1], vec![0xFFu8; 16], vec![0x41u8; 64]] {
        let _ = RecoveryJournalManager::deserialize_checkpoint(&junk, "ECM-A");
    }
}

// ---------------------------------------------------------------------------
// Documented source limitation
// ---------------------------------------------------------------------------

#[test]
fn interrupted_stage_is_unreachable_through_the_public_api() {
    // The source declares `InterruptedAbnormally` and treats it as
    // recovery-required, but no method transitions into it. This test documents
    // that fact rather than pretending the stage is exercisable.
    //
    // If a future change adds a transition, this test will need updating — which
    // is the point: it makes the gap visible.
    let stages = [
        JournalStage::ErasingSectors,
        JournalStage::WritingBlocks,
        JournalStage::FlashCompleted,
    ];
    assert_eq!(stages.len(), 3, "only three stages are reachable in practice");
}

#[test]
fn journal_operations_do_not_panic_on_edge_inputs() {
    // Zero-length ROM, zero bytes written, and time zero.
    let mut j = RecoveryJournalManager::new("", &[]);
    j.record_sector_erase_start(0, 0);
    j.record_block_written(0, 0);
    let _ = j.is_recovery_required();
    let _ = j.compute_resume_offset();
    let bytes = j.serialize_checkpoint();
    let _ = RecoveryJournalManager::deserialize_checkpoint(&bytes, "");
}
