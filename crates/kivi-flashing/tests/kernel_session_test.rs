//! Tests for the kernel-session contract.
//!
//! `KernelSession` is the second seam: it handles RAM-resident kernel commands.
//! Splitting it from `Device` matters because the two fail
//! differently — a device failure means the cable is gone, a session failure
//! means the ECU rejected a step and may need a specific recovery.

use kivi_flashing::device::{Device, DeviceKind, MockDevice, TransportKind};
use kivi_flashing::j2534_types::ProtocolId;
use kivi_flashing::kernel_session::{
    KernelError, KernelOperation, KernelSession, MockKernelSession, SessionPhase,
};

fn mock_device() -> MockDevice {
    let mut d = MockDevice::new(DeviceKind::Mock, "Mock", TransportKind::InMemory);
    d.open().unwrap();
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();
    d
}

fn ready_session() -> (MockDevice, MockKernelSession) {
    let dev = mock_device();
    let mut s = MockKernelSession::new(16 * 1024);
    s.set_phase(SessionPhase::Idle);
    (dev, s)
}

// ---------------------------------------------------------------------------
// Phases
// ---------------------------------------------------------------------------

#[test]
fn a_session_starts_idle() {
    let s = MockKernelSession::new(1024);
    assert_eq!(s.phase(), SessionPhase::Idle);
    assert!(!s.is_kernel_running());
}

#[test]
fn start_moves_the_session_to_kernel_running() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Read).expect("start");
    assert_eq!(s.phase(), SessionPhase::KernelRunning);
    assert!(s.is_kernel_running());
}

#[test]
fn start_records_the_requested_operation() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Write).expect("start");
    assert_eq!(s.operation(), Some(KernelOperation::Write));
}

#[test]
fn reading_requires_the_kernel_to_be_running() {
    let (_dev, mut s) = ready_session();
    let err = s.read_memory_block(0x0000, 256).expect_err("no kernel yet");
    assert_eq!(err, KernelError::KernelNotRunning);
}

#[test]
fn reading_after_start_returns_the_scripted_block() {
    let (_dev, mut s) = ready_session();
    s.set_block_data(0x1000, vec![0xAA; 64]);
    s.start(KernelOperation::Read).expect("start");

    let block = s.read_memory_block(0x1000, 64).expect("read");
    assert_eq!(block, vec![0xAA; 64]);
}

#[test]
fn reading_an_unscripted_address_fails_loudly() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Read).expect("start");
    let err = s
        .read_memory_block(0xDEAD_BEEF, 16)
        .expect_err("unscripted read must fail, not return zeros");
    assert_eq!(err, KernelError::NoDataForAddress(0xDEAD_BEEF));
}

#[test]
fn reading_the_wrong_length_is_rejected() {
    let (_dev, mut s) = ready_session();
    s.set_block_data(0x1000, vec![0xAA; 64]);
    s.start(KernelOperation::Read).expect("start");
    let err = s
        .read_memory_block(0x1000, 32)
        .expect_err("a short block must not be silently accepted");
    assert_eq!(err, KernelError::LengthMismatch { expected: 32, got: 64 });
}

// ---------------------------------------------------------------------------
// Erase / write / verify
// ---------------------------------------------------------------------------

#[test]
fn erase_requires_the_kernel_to_be_running() {
    let (_dev, mut s) = ready_session();
    assert_eq!(
        s.erase_range(0x0000, 0x1000).expect_err("no kernel"),
        KernelError::KernelNotRunning
    );
}

#[test]
fn erase_records_the_range() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Write).expect("start");
    s.erase_range(0x8000, 0x4000).expect("erase");
    assert_eq!(s.erased_ranges(), vec![(0x8000, 0x4000)]);
}

#[test]
fn writing_records_the_bytes() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Write).expect("start");
    s.write_range(0x8000, &[0x11, 0x22, 0x33]).expect("write");
    assert_eq!(s.written_bytes(), vec![(0x8000u32, vec![0x11, 0x22, 0x33])]);
}

#[test]
fn verify_passes_when_the_image_matches_what_was_written() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Write).expect("start");
    s.write_range(0x8000, &[0x11, 0x22]).unwrap();
    assert!(s.verify_image(0x8000, &[0x11, 0x22]).expect("verify"));
}

#[test]
fn verify_fails_when_a_byte_differs() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Write).expect("start");
    s.write_range(0x8000, &[0x11, 0x22]).unwrap();
    assert!(!s.verify_image(0x8000, &[0x11, 0x23]).expect("verify"));
}

#[test]
fn verify_reports_the_first_differing_offset() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Write).expect("start");
    s.write_range(0x8000, &[0x11, 0x22, 0x33]).unwrap();
    let mismatches = s.compare_image(0x8000, &[0x11, 0xFF, 0x33]).expect("compare");
    assert_eq!(mismatches, vec![(0x8001, 0x22, 0xFF)]);
}

// ---------------------------------------------------------------------------
// Cleanup — the guarantee that keeps an ECU alive
// ---------------------------------------------------------------------------

#[test]
fn cleanup_returns_the_session_to_idle() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Read).unwrap();
    s.cleanup().expect("cleanup");
    assert_eq!(s.phase(), SessionPhase::Idle);
    assert!(!s.is_kernel_running());
}

#[test]
fn cleanup_is_idempotent() {
    // Cleanup runs on every teardown path, including ones that follow failure.
    let (_dev, mut s) = ready_session();
    s.cleanup().expect("first cleanup");
    s.cleanup().expect("second cleanup must not fail");
    assert_eq!(s.phase(), SessionPhase::Idle);
}

#[test]
fn cleanup_after_a_failed_start_still_succeeds() {
    let (_dev, mut s) = ready_session();
    s.fail_start_with(KernelError::UploadRejected);
    let _ = s.start(KernelOperation::Read);
    s.cleanup().expect("cleanup must succeed even after a failed start");
    assert_eq!(s.phase(), SessionPhase::Idle);
}

#[test]
fn a_failed_start_leaves_the_session_recoverable() {
    let (_dev, mut s) = ready_session();
    s.fail_start_with(KernelError::UploadRejected);
    assert_eq!(
        s.start(KernelOperation::Read).expect_err("start"),
        KernelError::UploadRejected
    );
    // A retry must be possible without a cleanup in between.
    s.clear_start_failure();
    s.start(KernelOperation::Read).expect("retry");
}

#[test]
fn cleanup_records_that_it_ran() {
    let (_dev, mut s) = ready_session();
    assert_eq!(s.cleanup_count(), 0);
    s.cleanup().unwrap();
    assert_eq!(s.cleanup_count(), 1);
}

// ---------------------------------------------------------------------------
// Keep-alive
// ---------------------------------------------------------------------------

#[test]
fn keep_alive_requires_a_running_kernel() {
    let (_dev, mut s) = ready_session();
    assert_eq!(
        s.keep_alive().expect_err("no kernel"),
        KernelError::KernelNotRunning
    );
}

#[test]
fn keep_alive_counts_after_start() {
    let (_dev, mut s) = ready_session();
    s.start(KernelOperation::Read).unwrap();
    s.keep_alive().unwrap();
    s.keep_alive().unwrap();
    assert_eq!(s.keep_alive_count(), 2);
}

// ---------------------------------------------------------------------------
// Object safety
// ---------------------------------------------------------------------------

#[test]
fn the_kernel_session_trait_is_object_safe() {
    let s = MockKernelSession::new(1024);
    let boxed: Box<dyn KernelSession> = Box::new(s);
    assert_eq!(boxed.phase(), SessionPhase::Idle);
}

#[test]
fn the_session_reports_the_block_size_it_was_built_for() {
    let s = MockKernelSession::new(4096);
    assert_eq!(s.image_size(), 4096);
}
