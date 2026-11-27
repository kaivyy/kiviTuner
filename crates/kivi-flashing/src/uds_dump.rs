//! Dump a ROM over standard J2534, ELM327, or DoIP transports using UDS service 0x23.
//!
//! ## Why this path exists
//!
//! ISO 14229 **service 0x23 (`ReadMemoryByAddress`)** allows reading ECU memory over
//! standard J2534 PassThru, ELM327/STN, or DoIP adapters without uploading a custom RAM
//! kernel when the controller permits extended diagnostic reads. kiviTuner prefers
//! standard UDS readback first and falls back to a RAM kernel only when required.
//!
//! ## Why the driver sends raw UDS
//!
//! The segmentation is done by the transport, not here:
//!
//! | Transport | Who frames ISO-TP |
//! |---|---|
//! | J2534 configured for `ISO15765` | the interface |
//! | ELM327 / STN | the adapter |
//!
//! So this driver sends the raw UDS bytes and lets the transport carry them.
//! That is what makes one driver work across all of them.
//!
//! ## What this does NOT do
//!
//! It does not open a session, unlock security, or upload a kernel. Those are
//! separate steps the caller sequences; keeping them out means a failure in one
//! cannot be mistaken for a failure in another.

use crate::device::{Device, DeviceError};
use crate::j2534_types::{PassThruMsg, ProtocolId};

/// Default number of times a `0x78` pending response is tolerated per block.
pub const DEFAULT_MAX_PENDING_RETRIES: usize = 20;

/// How to perform the dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdsDumpOptions {
    /// Address of the first byte to read.
    pub start_address: u32,
    /// Total number of bytes to read.
    pub length: usize,
    /// Bytes requested per `0x23` call.
    pub block_size: usize,
    /// How many `0x78` pending responses to tolerate per block before giving up.
    pub max_pending_retries: usize,
    /// Protocol to send the messages on.
    pub protocol: ProtocolId,
    /// Physical tester CAN arbitration ID used when `device.uses_can_id_prefix()` is true.
    pub tx_can_id: u32,
}

impl UdsDumpOptions {
    /// Options with the default pending-retry budget.
    pub fn new(start_address: u32, length: usize, block_size: usize) -> Self {
        Self {
            start_address,
            length,
            block_size,
            max_pending_retries: DEFAULT_MAX_PENDING_RETRIES,
            protocol: ProtocolId::Iso15765,
            tx_can_id: 0x07E0,
        }
    }

    pub fn with_tx_can_id(mut self, tx_can_id: u32) -> Self {
        self.tx_can_id = tx_can_id;
        self
    }
}

/// What went wrong during a dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UdsDumpError {
    /// The options describe an impossible request.
    InvalidOptions,
    /// The transport failed.
    Device(DeviceError),
    /// The ECU sent nothing.
    EmptyResponse,
    /// The ECU replied with a negative response code.
    NegativeResponse { service: u8, nrc: u8 },
    /// The ECU replied `0x78` (requestCorrectlyReceived-ResponsePending).
    ///
    /// Not a failure: the driver waits and asks again, up to the retry budget.
    ResponsePending,
    /// The ECU replied `0x78` more times than the budget allows.
    TooManyPendingRetries { retries: usize },
    /// The ECU returned fewer bytes than requested.
    ///
    /// Never treated as a truncation: a short image written back would brick the
    /// ECU, so the dump stops here instead.
    ShortBlock { expected: usize, got: usize },
    /// The reply was not a response to service 0x23.
    UnexpectedService { got: u8 },
}

impl From<DeviceError> for UdsDumpError {
    fn from(e: DeviceError) -> Self {
        UdsDumpError::Device(e)
    }
}

impl std::fmt::Display for UdsDumpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UdsDumpError::InvalidOptions => {
                f.write_str("dump options are invalid (zero length or zero block size)")
            }
            UdsDumpError::Device(e) => write!(f, "transport: {e}"),
            UdsDumpError::EmptyResponse => f.write_str("the ECU sent an empty reply"),
            UdsDumpError::NegativeResponse { service, nrc } => write!(
                f,
                "the ECU rejected service {service:#04x} with NRC {nrc:#04x}"
            ),
            UdsDumpError::ResponsePending => {
                f.write_str("the ECU is busy (response pending)")
            }
            UdsDumpError::TooManyPendingRetries { retries } => write!(
                f,
                "the ECU kept replying 'response pending' {retries} times; giving up"
            ),
            UdsDumpError::ShortBlock { expected, got } => write!(
                f,
                "the ECU returned {got} bytes where {expected} were requested; \
                 a short image would brick the ECU if written back"
            ),
            UdsDumpError::UnexpectedService { got } => {
                write!(f, "expected a reply to service 0x23, got {got:#04x}")
            }
        }
    }
}

impl std::error::Error for UdsDumpError {}

/// Result of a successful dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdsDumpOutcome {
    pub base_address: u32,
    pub bytes: Vec<u8>,
    pub blocks_read: usize,
    /// Total `0x78` pending responses absorbed across the whole dump.
    pub pending_retries: usize,
}

/// Encode a `0x23 ReadMemoryByAddress` request.
///
/// ISO 14229-1 layout:
///
/// ```text
/// 0x23 | addressAndLengthFormatIdentifier | memoryAddress | memorySize
/// ```
///
/// `0x24` means: high nibble 2 → the size field is 2 bytes; low nibble 4 → the
/// address field is 4 bytes.
pub fn build_read_request(address: u32, length: usize) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(8);
    pkt.push(0x23);
    pkt.push(0x24);
    pkt.extend_from_slice(&address.to_be_bytes());
    pkt.extend_from_slice(&(length as u16).to_be_bytes());
    pkt
}

/// Parse a reply to a `0x23` request.
///
/// A positive reply is `0x63` followed by the data. `0x63 = 0x23 + 0x40`.
pub fn parse_read_response(reply: &[u8]) -> Result<Vec<u8>, UdsDumpError> {
    if reply.is_empty() {
        return Err(UdsDumpError::EmptyResponse);
    }

    // Negative response: 0x7F | service | NRC
    if reply[0] == 0x7F {
        if reply.len() < 3 {
            return Err(UdsDumpError::EmptyResponse);
        }
        let service = reply[1];
        let nrc = reply[2];
        if nrc == 0x78 {
            return Err(UdsDumpError::ResponsePending);
        }
        return Err(UdsDumpError::NegativeResponse { service, nrc });
    }

    if reply[0] != 0x63 {
        return Err(UdsDumpError::UnexpectedService { got: reply[0] });
    }

    Ok(reply[1..].to_vec())
}

/// Read `[start_address, start_address + length)` over UDS 0x23, reporting
/// `(bytes_done, total_bytes, blocks_read, pending_retries)` after each block.
///
/// Blocks are requested in `block_size` chunks. A `0x78` pending response is
/// retried up to `max_pending_retries` times per block; anything else stops the
/// dump immediately.
pub fn dump_via_uds23_with_progress<D: Device + ?Sized, F>(
    device: &mut D,
    opts: &UdsDumpOptions,
    mut on_progress: F,
) -> Result<UdsDumpOutcome, UdsDumpError>
where
    F: FnMut(usize, usize, usize, usize),
{
    if opts.length == 0 || opts.block_size == 0 || opts.block_size > u16::MAX as usize {
        return Err(UdsDumpError::InvalidOptions);
    }
    let last_offset = u32::try_from(opts.length - 1).map_err(|_| UdsDumpError::InvalidOptions)?;
    if opts.start_address.checked_add(last_offset).is_none() {
        return Err(UdsDumpError::InvalidOptions);
    }

    let use_can_prefix =
        device.uses_can_id_prefix() && matches!(opts.protocol, ProtocolId::Iso15765 | ProtocolId::Can);

    let mut bytes = Vec::with_capacity(opts.length);
    let mut blocks_read = 0usize;
    let mut pending_total = 0usize;
    let mut offset = 0usize;

    while offset < opts.length {
        let want = (opts.length - offset).min(opts.block_size);
        let address = opts
            .start_address
            .checked_add(offset as u32)
            .ok_or(UdsDumpError::InvalidOptions)?;

        let request = build_read_request(address, want);
        let (flags, wire) = if use_can_prefix {
            let mut tx_flags_val = 0u32;
            if opts.protocol.is_iso15765() {
                tx_flags_val |= crate::j2534_types::tx_flags::ISO15765_FRAME_PAD;
            }
            if opts.tx_can_id > 0x7FF {
                tx_flags_val |= crate::j2534_types::tx_flags::CAN_29BIT_ID;
            }
            let mut v = Vec::with_capacity(4 + request.len());
            v.extend_from_slice(&opts.tx_can_id.to_be_bytes());
            v.extend_from_slice(&request);
            (tx_flags_val, v)
        } else {
            (0u32, request)
        };
        device.send(&PassThruMsg::with_flags(opts.protocol, flags, wire))?;

        // Absorb pending responses for this block.
        let mut pending_here = 0usize;
        let data = loop {
            let reply = device.receive()?;
            let payload = if use_can_prefix {
                if reply.data.len() < 4 {
                    return Err(UdsDumpError::EmptyResponse);
                }
                &reply.data[4..]
            } else {
                reply.data.as_slice()
            };
            match parse_read_response(payload) {
                Ok(data) => break data,
                Err(UdsDumpError::ResponsePending) => {
                    pending_here += 1;
                    pending_total += 1;
                    if pending_here > opts.max_pending_retries {
                        return Err(UdsDumpError::TooManyPendingRetries {
                            retries: pending_here,
                        });
                    }
                    // ISO 14229-1 §7.5.5: on NRC 0x78 the ECU already accepted
                    // the request and will send the final reply without re-transmission.
                }
                Err(e) => return Err(e),
            }
        };

        if data.len() < want {
            return Err(UdsDumpError::ShortBlock {
                expected: want,
                got: data.len(),
            });
        }

        // A reply may carry more than asked; take exactly what was requested so
        // the image length is exactly `opts.length`.
        bytes.extend_from_slice(&data[..want]);
        blocks_read += 1;
        offset += want;
        on_progress(offset, opts.length, blocks_read, pending_total);
    }

    Ok(UdsDumpOutcome {
        base_address: opts.start_address,
        bytes,
        blocks_read,
        pending_retries: pending_total,
    })
}

/// Read `[start_address, start_address + length)` over UDS 0x23.
///
/// Blocks are requested in `block_size` chunks. A `0x78` pending response is
/// retried up to `max_pending_retries` times per block; anything else stops the
/// dump immediately.
pub fn dump_via_uds23<D: Device + ?Sized>(
    device: &mut D,
    opts: &UdsDumpOptions,
) -> Result<UdsDumpOutcome, UdsDumpError> {
    dump_via_uds23_with_progress(device, opts, |_, _, _, _| {})
}
