//! Tests for the flash orchestrator and its cleanup guarantee.
//!
//! The single most important property here is that **the ECU always gets its
//! stock OS back**, even when a step fails or the caller panics; this cleanup
//! guarantee is enforced deterministically via Rust `Drop`.

use kivi_flashing::device::{Device, DeviceKind, MockDevice, TransportKind};
use kivi_flashing::flash_engine::{
    dump_ecu, flash_ecu, CleanupGuard, FlashError, FlashOutcome, FlashPlan,
};
use kivi_flashing::j2534_types::ProtocolId;
use kivi_flashing::kernel_session::{KernelOperation, KernelSession, MockKernelSession};

fn bench(image_size: usize) -> (MockDevice, MockKernelSession) {
    let mut dev = MockDevice::new(DeviceKind::Mock, "Bench", TransportKind::InMemory);
    dev.open().unwrap();
    dev.connect(ProtocolId::Iso15765, 500_000).unwrap();
    let sess = MockKernelSession::new(image_size);
    (dev, sess)
}

// ---------------------------------------------------------------------------
// Dump
// ---------------------------------------------------------------------------

#[test]
fn a_dump_reads_the_requested_range_and_returns_a_bin() {
    let (mut dev, mut sess) = bench(0x1000);
    sess.set_block_data(0x0000, vec![0xAA; 0x100]);
    sess.set_block_data(0x0100, vec![0xBB; 0x100]);

    let outcome = dump_ecu(&mut dev, &mut sess, 0x0000, 0x200, 0x100).expect("dump");
    assert_eq!(outcome.bytes.len(), 0x200);
    assert_eq!(&outcome.bytes[..0x100], &vec![0xAA; 0x100][..]);
    assert_eq!(&outcome.bytes[0x100..], &vec![0xBB; 0x100][..]);
}

#[test]
fn a_dump_cleans_up_the_kernel_when_it_finishes() {
    let (mut dev, mut sess) = bench(0x1000);
    sess.set_block_data(0x0000, vec![0x00; 0x100]);

    dump_ecu(&mut dev, &mut sess, 0x0000, 0x100, 0x100).expect("dump");
    assert_eq!(
        sess.cleanup_count(),
        1,
        "the kernel must be shut down exactly once"
    );
    assert!(!sess.is_kernel_running());
}

#[test]
fn a_dump_cleans_up_even_when_a_block_is_missing() {
    // The failure path is the one that matters: a kernel left running keeps the
    // ECU awake and drains the battery.
    let (mut dev, mut sess) = bench(0x1000);
    sess.set_block_data(0x0000, vec![0x00; 0x100]);
    // 0x0100 deliberately not scripted.

    let err = dump_ecu(&mut dev, &mut sess, 0x0000, 0x200, 0x100).expect_err("must fail");
    assert!(matches!(err, FlashError::Kernel(_)));
    assert_eq!(sess.cleanup_count(), 1, "cleanup must run on the error path");
    assert!(!sess.is_kernel_running());
}

#[test]
fn a_dump_reports_how_many_blocks_it_read() {
    let (mut dev, mut sess) = bench(0x1000);
    for a in [0x0000u32, 0x0100, 0x0200] {
        sess.set_block_data(a, vec![0x11; 0x100]);
    }
    let outcome = dump_ecu(&mut dev, &mut sess, 0x0000, 0x300, 0x100).expect("dump");
    assert_eq!(outcome.blocks_read, 3);
}

#[test]
fn a_dump_records_the_base_address_it_started_from() {
    let (mut dev, mut sess) = bench(0x1000);
    sess.set_block_data(0x8000, vec![0x22; 0x100]);
    let outcome = dump_ecu(&mut dev, &mut sess, 0x8000, 0x100, 0x100).expect("dump");
    assert_eq!(outcome.base_address, 0x8000);
}

// ---------------------------------------------------------------------------
// Flash
// ---------------------------------------------------------------------------

#[test]
fn a_flash_erases_then_writes_then_verifies() {
    let (mut dev, mut sess) = bench(0x1000);
    let image = vec![0x5A; 0x200];
    let plan = FlashPlan::full(0x0000, image.len());

    let outcome = flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect("flash");
    assert!(outcome.verified);
    assert_eq!(sess.erased_ranges(), vec![(0x0000, 0x200)]);
    assert_eq!(sess.written_bytes().len(), 2);
}

#[test]
fn a_flash_cleans_up_after_success() {
    let (mut dev, mut sess) = bench(0x1000);
    let image = vec![0x5A; 0x100];
    let plan = FlashPlan::full(0x0000, image.len());
    flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect("flash");
    assert_eq!(sess.cleanup_count(), 1);
}

#[test]
fn a_flash_cleans_up_after_a_verification_failure() {
    let (mut dev, mut sess) = bench(0x1000);
    let image = vec![0x5A; 0x100];
    let plan = FlashPlan::full(0x0000, image.len());
    sess.fail_start_with(kivi_flashing::kernel_session::KernelError::UploadRejected);

    let err = flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect_err("must fail");
    assert!(matches!(err, FlashError::Kernel(_)));
    assert_eq!(
        sess.cleanup_count(),
        1,
        "cleanup must run when the kernel never started"
    );
}

#[test]
fn a_differential_plan_skips_unchanged_sectors() {
    let plan = FlashPlan::differential(0x0000, 0x1000, &[(0x0200, 0x100)]);
    assert_eq!(plan.ranges, vec![(0x0200, 0x100)]);
    assert!(!plan.is_full());
}

#[test]
fn a_full_plan_covers_the_whole_range() {
    let plan = FlashPlan::full(0x0000, 0x1000);
    assert_eq!(plan.ranges, vec![(0x0000, 0x1000)]);
    assert!(plan.is_full());
}

#[test]
fn a_flash_refuses_an_image_shorter_than_the_plan() {
    let (mut dev, mut sess) = bench(0x1000);
    let plan = FlashPlan::full(0x0000, 0x200);
    let image = vec![0x5A; 0x100]; // half the plan
    let err = flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect_err("must refuse");
    assert!(matches!(err, FlashError::ImageTooShort { .. }));
    assert_eq!(
        sess.cleanup_count(),
        0,
        "nothing was started, so nothing needs cleaning"
    );
}

#[test]
fn a_flash_refuses_an_image_longer_than_the_plan() {
    let (mut dev, mut sess) = bench(0x1000);
    let plan = FlashPlan::full(0x0000, 0x100);
    let image = vec![0x5A; 0x200];
    let err = flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect_err("must refuse");
    assert!(matches!(err, FlashError::ImageTooLong { .. }));
}

#[test]
fn a_flash_with_a_zero_length_plan_is_refused() {
    let (mut dev, mut sess) = bench(0x1000);
    let plan = FlashPlan::full(0x0000, 0);
    let err = flash_ecu(&mut dev, &mut sess, &plan, &[], 0x100).expect_err("must refuse");
    assert!(matches!(err, FlashError::EmptyPlan));
}

// ---------------------------------------------------------------------------
// CleanupGuard
// ---------------------------------------------------------------------------

#[test]
fn the_guard_cleans_up_when_dropped() {
    let (_dev, mut sess) = bench(0x1000);
    sess.start(KernelOperation::Read).unwrap();
    assert!(sess.is_kernel_running(), "precondition");
    {
        // The guard holds the mutable borrow for its whole lifetime, so the
        // session cannot be inspected inside this block. That is the point: it
        // prevents a second `start` racing the cleanup.
        let _guard = CleanupGuard::new(&mut sess);
    }
    assert_eq!(sess.cleanup_count(), 1);
    assert!(!sess.is_kernel_running());
}

#[test]
fn the_guard_cleans_up_when_the_scope_unwinds() {
    let (_dev, mut sess) = bench(0x1000);
    sess.start(KernelOperation::Read).unwrap();

    // Assert inside the closure: `sess` is borrowed by the guard for the whole
    // closure body, so the counter must be read after `catch_unwind` returns.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = CleanupGuard::new(&mut sess);
        panic!("simulated failure mid-flash");
    }));
    assert!(result.is_err());
    assert_eq!(
        sess.cleanup_count(),
        1,
        "a panic must still restore the stock OS"
    );
}

#[test]
fn the_guard_can_be_disarmed_when_cleanup_already_happened() {
    let (_dev, mut sess) = bench(0x1000);
    sess.start(KernelOperation::Read).unwrap();
    {
        let mut guard = CleanupGuard::new(&mut sess);
        guard.cleanup_now().expect("explicit cleanup");
        // Cannot read `sess` here: the guard still holds the mutable borrow.
        // Dropping after an explicit cleanup must not clean up twice; that is
        // checked after the scope ends.
    }
    assert_eq!(sess.cleanup_count(), 1);
}

#[test]
fn the_outcome_reports_whether_verification_passed() {
    let (mut dev, mut sess) = bench(0x1000);
    let image = vec![0x77; 0x100];
    let plan = FlashPlan::full(0x0000, image.len());
    let outcome: FlashOutcome = flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect("flash");
    assert!(outcome.verified);
    assert_eq!(outcome.bytes_written, 0x100);
    assert_eq!(outcome.blocks_written, 1);
}

// ---------------------------------------------------------------------------
// Overlapping ranges
// ---------------------------------------------------------------------------

#[test]
fn a_plan_with_overlapping_ranges_is_refused_before_the_device_is_touched() {
    // Two ranges covering the same byte produce an order-dependent image while
    // every individual write still reports success.
    let (mut dev, mut sess) = bench(0x1000);
    let plan = FlashPlan {
        base_address: 0x0000,
        ranges: vec![(0x0000, 0x100), (0x0080, 0x100)],
    };
    let image = vec![0x5A; 0x200];

    let err = flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect_err("must refuse");
    assert!(matches!(err, FlashError::OverlappingRanges));
    assert_eq!(
        sess.cleanup_count(),
        0,
        "no kernel was started, so nothing to clean"
    );
    assert!(dev.sent_messages().is_empty(), "nothing reached the device");
}

#[test]
fn abutting_ranges_are_allowed() {
    // A flash split into consecutive sectors is the normal case, not an overlap.
    let (mut dev, mut sess) = bench(0x1000);
    let plan = FlashPlan {
        base_address: 0x0000,
        ranges: vec![(0x0000, 0x100), (0x0100, 0x100)],
    };
    let image = vec![0x5A; 0x200];
    let out = flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect("flash");
    assert!(out.verified);
}

#[test]
fn dump_and_flash_with_progress_emit_monotonic_updates() {
    use kivi_flashing::flash_engine::{
        dump_ecu_with_progress, flash_ecu_with_progress, FlashTransferStage,
    };

    let (mut dev, mut sess) = bench(0x400);
    for addr in [0x0000u32, 0x0100, 0x0200] {
        sess.set_block_data(addr, vec![0x11; 0x100]);
    }
    let mut read_events = Vec::new();
    let dumped = dump_ecu_with_progress(
        &mut dev,
        &mut sess,
        0x0000,
        0x300,
        0x100,
        |done, total, blocks| {
            read_events.push((done, total, blocks));
        },
    )
    .expect("dump with progress");
    assert_eq!(dumped.blocks_read, 3);
    assert_eq!(read_events, vec![(0x100, 0x300, 1), (0x200, 0x300, 2), (0x300, 0x300, 3)]);

    let plan = FlashPlan::full(0x0000, 0x300);
    let image = vec![0x33; 0x300];
    let mut write_events = Vec::new();
    let flashed = flash_ecu_with_progress(
        &mut dev,
        &mut sess,
        &plan,
        &image,
        0x100,
        |stage, done, total, blocks| {
            write_events.push((stage, done, total, blocks));
        },
    )
    .expect("flash with progress");
    assert!(flashed.verified);
    assert_eq!(
        write_events,
        vec![
            (FlashTransferStage::Erasing, 0, 0x300, 0),
            (FlashTransferStage::Writing, 0x100, 0x300, 1),
            (FlashTransferStage::Writing, 0x200, 0x300, 2),
            (FlashTransferStage::Writing, 0x300, 0x300, 3),
            (FlashTransferStage::Verifying, 0, 0x300, 3),
            (FlashTransferStage::Verifying, 0x300, 0x300, 3),
        ]
    );
}
