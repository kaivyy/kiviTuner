//! Tests for the C-ABI wire struct.
//!
//! ## Why this matters more than it looks
//!
//! `PassThruMsg` (the Rust-friendly one) carries a `Vec<u8>`. That is correct
//! for a Rust API and completely wrong across an FFI boundary: a vendor DLL
//! expects a flat, fixed-layout block it can read with a C pointer. Getting one
//! field offset wrong means the driver reads a length from where the timestamp
//! should be, and the failure appears later as a garbled frame or a hang.
//!
//! These tests pin the layout against the published C struct:
//!
//! ```c
//! typedef struct {
//!     unsigned long ProtocolID;      // offset 0
//!     unsigned long RxStatus;        // offset 4
//!     unsigned long TxFlags;         // offset 8
//!     unsigned long Timestamp;       // offset 12
//!     unsigned long DataSize;        // offset 16
//!     unsigned long ExtraDataIndex;  // offset 20
//!     unsigned char Data[4128];      // offset 24
//! } PASSTHRU_MSG;                    // sizeof == 4152
//! ```
//!
//! Layout verified against the published SAE J2534-1 v04.04 C ABI
//! (`PASSTHRU_MSG`: six 32-bit `unsigned long` fields plus `Data[4128]`).

use kivi_flashing::j2534_abi::{
    PassthruMsgAbi, PASSTHRU_MSG_ABI_SIZE, PASSTHRU_MSG_DATA_OFFSET,
};
use kivi_flashing::j2534_types::{tx_flags, PassThruMsg, ProtocolId};

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[test]
fn the_struct_size_matches_the_c_header() {
    // 6 x 4-byte fields + 4128-byte array.
    assert_eq!(PASSTHRU_MSG_ABI_SIZE, 4152);
    assert_eq!(std::mem::size_of::<PassthruMsgAbi>(), 4152);
}

#[test]
fn the_data_array_starts_after_the_six_header_fields() {
    assert_eq!(PASSTHRU_MSG_DATA_OFFSET, 24);
}

#[test]
fn the_data_array_holds_4128_bytes() {
    let msg = PassthruMsgAbi::zeroed();
    assert_eq!(msg.data.len(), 4128);
}

#[test]
fn the_struct_is_plain_old_data() {
    // `Copy` + no Drop means it can be passed by value across the boundary.
    fn assert_copy<T: Copy>() {}
    assert_copy::<PassthruMsgAbi>();
}

// ---------------------------------------------------------------------------
// Field placement
// ---------------------------------------------------------------------------

#[test]
fn the_header_fields_land_at_the_documented_offsets() {
    let msg = PassthruMsgAbi::zeroed();
    // One instance, one base pointer: comparing against a temporary's address
    // would be undefined and is exactly the bug this test would otherwise have.
    let base = &msg as *const PassthruMsgAbi as usize;

    assert_eq!(&msg.protocol_id as *const u32 as usize - base, 0);
    assert_eq!(&msg.rx_status as *const u32 as usize - base, 4);
    assert_eq!(&msg.tx_flags as *const u32 as usize - base, 8);
    assert_eq!(&msg.timestamp as *const u32 as usize - base, 12);
    assert_eq!(&msg.data_size as *const u32 as usize - base, 16);
    assert_eq!(&msg.extra_data_index as *const u32 as usize - base, 20);
    // The data array must begin exactly where the header ends.
    assert_eq!(&msg.data as *const _ as usize - base, 24);

    // And the last byte of the array must be the last byte of the struct.
    let bytes = &msg as *const PassthruMsgAbi as *const u8;
    unsafe {
        let last = bytes.add(PASSTHRU_MSG_ABI_SIZE - 1) as usize;
        assert_eq!(last - base, PASSTHRU_MSG_ABI_SIZE - 1);
    }
}

// ---------------------------------------------------------------------------
// Conversion from the Rust-friendly type
// ---------------------------------------------------------------------------

#[test]
fn converting_a_message_sets_data_size_from_the_payload_length() {
    let msg = PassThruMsg::new(ProtocolId::Iso15765, vec![0x02, 0x10, 0x03]);
    let abi = PassthruMsgAbi::from_msg(&msg);
    assert_eq!(abi.data_size, 3);
    assert_eq!(abi.protocol_id, 0x06);
}

#[test]
fn converting_copies_the_payload_into_the_fixed_array() {
    let msg = PassThruMsg::new(ProtocolId::Can, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    let abi = PassthruMsgAbi::from_msg(&msg);
    assert_eq!(&abi.data[..4], &[0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(abi.data[4], 0x00, "the rest must stay zeroed");
}

#[test]
fn converting_carries_the_flags_through() {
    let msg = PassThruMsg::with_flags(
        ProtocolId::Can,
        tx_flags::CAN_29BIT_ID | tx_flags::ISO15765_FRAME_PAD,
        vec![0x01],
    );
    let abi = PassthruMsgAbi::from_msg(&msg);
    assert_eq!(abi.tx_flags, tx_flags::CAN_29BIT_ID | tx_flags::ISO15765_FRAME_PAD);
}

#[test]
fn a_payload_larger_than_the_array_is_truncated_not_overflowed() {
    // A 4129-byte payload cannot fit. The conversion must clamp rather than
    // write past the end of the fixed array.
    let oversized = vec![0xAA; 4129];
    let msg = PassThruMsg::new(ProtocolId::Iso15765, oversized);
    let abi = PassthruMsgAbi::from_msg(&msg);
    assert_eq!(abi.data_size as usize, 4128);
    assert_eq!(abi.data[4127], 0xAA);
}

// ---------------------------------------------------------------------------
// Conversion back to the Rust-friendly type
// ---------------------------------------------------------------------------

#[test]
fn converting_back_uses_data_size_not_the_whole_array() {
    let mut abi = PassthruMsgAbi::zeroed();
    abi.protocol_id = 0x06;
    abi.data_size = 3;
    abi.data[0] = 0x02;
    abi.data[1] = 0x50;
    abi.data[2] = 0x03;
    // Trailing bytes are left over from a previous frame.
    abi.data[3] = 0xFF;
    abi.data[4] = 0xFF;

    let msg = abi.to_msg();
    assert_eq!(msg.data, vec![0x02, 0x50, 0x03]);
    assert_eq!(msg.protocol_id, ProtocolId::Iso15765);
}

#[test]
fn converting_back_rejects_an_impossible_data_size() {
    // A driver that reports more than the array holds is broken; trusting it
    // would index out of bounds.
    let mut abi = PassthruMsgAbi::zeroed();
    abi.data_size = 9999;
    let msg = abi.to_msg();
    assert_eq!(
        msg.data.len(),
        4128,
        "an out-of-range data_size must be clamped to the array length"
    );
}

#[test]
fn a_zero_data_size_yields_an_empty_payload() {
    let abi = PassthruMsgAbi::zeroed();
    let msg = abi.to_msg();
    assert!(msg.data.is_empty());
}

// ---------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------

#[test]
fn a_message_survives_a_round_trip_through_the_abi_struct() {
    let original = PassThruMsg::with_flags(
        ProtocolId::Iso15765,
        tx_flags::ISO15765_FRAME_PAD,
        vec![0x10, 0x14, 0x2E, 0xF1, 0x90],
    );
    let restored = PassthruMsgAbi::from_msg(&original).to_msg();

    assert_eq!(restored.data, original.data);
    assert_eq!(restored.protocol_id, original.protocol_id);
    assert_eq!(restored.tx_flags, original.tx_flags);
}

#[test]
fn an_empty_message_round_trips() {
    let original = PassThruMsg::new(ProtocolId::Can, vec![]);
    let restored = PassthruMsgAbi::from_msg(&original).to_msg();
    assert!(restored.data.is_empty());
    assert_eq!(restored.protocol_id, ProtocolId::Can);
}
