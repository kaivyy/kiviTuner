//! ELM327 serial device — the cheap path to a real car.
//!
//! ## Why this exists alongside J2534
//!
//! A J2534 interface costs a few hundred dollars and needs a vendor DLL. An
//! ELM327 clone costs fifteen and speaks plain ASCII over a serial port. That
//! makes it the realistic first hardware for most people, and it is what
//! the reference implementation's `ELM327PlugIn.dll` uses.
//!
//! ## What an ELM327 can and cannot do
//!
//! | Can | Cannot |
//! |---|---|
//! | Read live data (Mode 01) | Flash — the AT-command bridge caps a request at 1020 ms |
//! | Read VIN / calibration ID (Mode 09) | Reach boot mode or a RAM kernel |
//! | Read and clear DTCs | Sustain the multi-KB block transfers a flash needs |
//!
//! [`ElmDevice::supports_flashing`] returns `false` and is tested, because
//! offering a flash button for an adapter that cannot do it is the most
//! dangerous lie this crate could tell.
//!
//! ## Protocol notes
//!
//! Initialization resets the AT bridge, silences echo/linefeeds/spaces, and
//! enables automatic OBD protocol detection (`ATSP0`) so the adapter works across
//! vehicles without pre-pinning a fixed bus protocol.
//!
//! ## Reference
//!
//! Command strings and the 1020 ms timeout ceiling (`AT ST FF` = `255 * 4 ms`)
//! follow the published ELM327 AT command datasheet.

/// ELM327-specific failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElmError {
    /// The adapter answered `NO DATA` — the ECU did not reply.
    NoData,
    /// The adapter answered `?` — unrecognised command.
    UnknownCommand,
    /// The adapter answered `STOPPED` — the user or a timeout halted it.
    Stopped,
    /// The reply could not be parsed as hex.
    MalformedReply(String),
    /// The port could not be opened.
    Port(String),
}

impl std::fmt::Display for ElmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElmError::NoData => f.write_str("the adapter reported NO DATA"),
            ElmError::UnknownCommand => f.write_str("the adapter did not recognise the command"),
            ElmError::Stopped => f.write_str("the adapter reported STOPPED"),
            ElmError::MalformedReply(m) => write!(f, "malformed reply: {m}"),
            ElmError::Port(m) => write!(f, "serial port failure: {m}"),
        }
    }
}

impl std::error::Error for ElmError {}

/// A parsed ELM327 reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElmResponse {
    /// The payload bytes, prompt and status lines removed.
    pub bytes: Vec<u8>,
    /// The cleaned text, for display and for `OK` checks.
    pub text: String,
    /// Which status word the adapter used.
    pub status: ElmStatus,
}

/// The status word an ELM327 prefixes a reply with, when it uses one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElmStatus {
    /// A normal reply carrying data or `OK`.
    Ok,
    /// `NO DATA`.
    NoData,
    /// `?`.
    Error,
    /// `STOPPED`.
    Stopped,
    /// `SEARCHING...` followed by data.
    Searching,
    /// `BUS INIT: ...` followed by data.
    BusInit,
}

// Status discriminants, exposed so tests can write `ElmResponse::NO_DATA`
// without importing `ElmStatus`. They are constants rather than variants so the
// comparison reads the way the adapter's output does.
impl ElmResponse {
    pub const NO_DATA: ElmStatus = ElmStatus::NoData;
    pub const ERROR: ElmStatus = ElmStatus::Error;
    pub const STOPPED: ElmStatus = ElmStatus::Stopped;
}

impl ElmResponse {
    /// Parse raw adapter output.
    ///
    /// Handles the shapes an ELM327 actually produces:
    ///
    /// - `"OK\r\r>"` — a successful configuration command
    /// - `"41 0C 1A F8\r\r>"` — a data reply
    /// - `"SEARCHING...\r41 0C 1A F8\r\r>"` — a data reply after a protocol search
    /// - `"NO DATA\r\r>"` — no ECU reply
    /// - `"?"\r\r>` — unrecognised command
    ///
    /// The `>` prompt is always stripped, and non-hex status lines are removed
    /// before hex decoding, so a `SEARCHING...` prefix does not corrupt the
    /// payload.
    pub fn parse(raw: &str) -> Self {
        // Strip NUL bytes and non-printable serial/CH340 framing noise emitted during reset,
        // normalise line endings, and drop the terminating prompt.
        let sanitized: String = raw
            .chars()
            .filter(|&c| c == '\r' || c == '\n' || c == '\t' || (' '..='~').contains(&c))
            .collect();
        let cleaned: String = sanitized
            .replace(['\r', '\n'], "\n")
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && *l != ">")
            .collect::<Vec<_>>()
            .join("\n");

        let upper = cleaned.to_ascii_uppercase();

        if upper.contains("NO DATA") {
            return Self {
                bytes: Vec::new(),
                text: "NO DATA".to_string(),
                status: ElmStatus::NoData,
            };
        }
        if upper.trim() == "?"
            || upper.contains("CAN ERROR")
            || upper.contains("BUS BUSY")
            || upper.contains("BUS ERROR")
            || upper.contains("UNABLE TO CONNECT")
            || upper.contains("DATA ERROR")
            || upper.contains("BUFFER FULL")
            || upper.contains("FB ERROR")
            || upper.contains("RX ERROR")
            || upper.contains("LV RESET")
            || upper.contains("ACT ALERT")
            || upper.contains("LP ALERT")
            || upper
                .split_whitespace()
                .any(|tok| tok == "ERROR" || tok.starts_with("ERR"))
        {
            return Self {
                bytes: Vec::new(),
                text: cleaned,
                status: ElmStatus::Error,
            };
        }
        if upper.contains("STOPPED") {
            return Self {
                bytes: Vec::new(),
                text: "STOPPED".to_string(),
                status: ElmStatus::Stopped,
            };
        }

        // Status lines that may precede a payload.
        let mut status = ElmStatus::Ok;
        let mut payload_lines: Vec<&str> = Vec::new();
        for line in cleaned.split('\n') {
            let u = line.to_ascii_uppercase();
            if u.starts_with("SEARCHING") {
                status = ElmStatus::Searching;
                continue;
            }
            if u.starts_with("BUS INIT") {
                status = ElmStatus::BusInit;
                continue;
            }
            payload_lines.push(line);
        }

        let has_isotp_seq = payload_lines.iter().any(|l| {
            l.find(':')
                .is_some_and(|c| (1..=2).contains(&c) && l[..c].chars().all(|ch| ch.is_ascii_hexdigit()))
        });
        let mut expected_isotp_len: Option<usize> = None;
        let normalized_lines: Vec<&str> = payload_lines
            .iter()
            .filter_map(|&l| {
                if has_isotp_seq {
                    if let Some(colon) = l
                        .find(':')
                        .filter(|&c| (1..=2).contains(&c) && l[..c].chars().all(|ch| ch.is_ascii_hexdigit()))
                    {
                        return Some(l[colon + 1..].trim());
                    }
                    if l.len() == 3 && l.chars().all(|ch| ch.is_ascii_hexdigit()) {
                        if let Ok(len) = usize::from_str_radix(l, 16) {
                            expected_isotp_len = Some(len);
                        }
                        return None;
                    }
                }
                Some(l)
            })
            .collect();

        // `OK` carries no payload.
        let text = normalized_lines.join(" ").trim().to_string();
        if text.eq_ignore_ascii_case("OK") {
            return Self {
                bytes: Vec::new(),
                text: "OK".to_string(),
                status,
            };
        }

        let mut bytes = parse_hex_payload(&text);
        if let Some(max_len) = expected_isotp_len {
            if max_len > 0 && bytes.len() > max_len {
                bytes.truncate(max_len);
            }
        }
        Self {
            bytes,
            text,
            status,
        }
    }

    /// Whether the adapter reported plain success.
    pub fn is_ok(&self) -> bool {
        self.text.eq_ignore_ascii_case("OK") && self.status == ElmStatus::Ok
    }
}

/// Decode hex bytes out of a reply, ignoring any non-hex character.
///
/// A reply can carry spaces (`41 0C 1A F8`) or not (`410C1AF8`), and some clones
/// append a colon or an index. Anything that is not a hex digit is skipped, and
/// an odd trailing digit is dropped rather than panicking.
fn parse_hex_payload(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text
        .chars()
        .filter_map(|c| c.to_digit(16).map(|d| d as u8))
        .collect();
    digits
        .chunks_exact(2)
        .map(|pair| (pair[0] << 4) | pair[1])
        .collect()
}

/// An ELM327 adapter.
///
/// The struct is a namespace for the pure parts of the protocol. A live instance
/// would hold a `SerialObdTransport`, which already exists in
/// `kivi-protocol::obd::serial`; wiring the two together needs a real port and is
/// deliberately not done here.
pub struct ElmDevice;

impl ElmDevice {
    /// The maximum timeout an ELM327 accepts, in milliseconds.
    ///
    /// Per the ELM327 AT command specification (`AT ST FF` = 255 * 4 ms),
    /// the programmable timeout ceiling is 1020 ms.
    pub const MAX_TIMEOUT_MS: u32 = 1020;

    /// The initialisation order, without the trailing carriage return.
    pub fn init_sequence() -> Vec<&'static str> {
        vec!["ATZ", "ATE0", "ATL0", "ATS0", "ATSP0"]
    }

    /// Encode a command for transmission.
    ///
    /// Trims surrounding whitespace and appends `\r`. An empty command becomes a
    /// bare carriage return, which wakes low-power adapters or clones that drop
    /// the first wakeup byte before `ATZ`.
    pub fn encode(cmd: &str) -> String {
        format!("{}\r", cmd.trim())
    }

    /// An ELM327 cannot flash an ECU. Always `false`.
    ///
    /// This is not a limitation to be worked around: the AT-command bridge
    /// cannot sustain the block transfers a flash needs, and the 1020 ms ceiling
    /// is below a single erase step. Reporting `true` would let the UI offer a
    /// button that bricks an ECU.
    pub fn supports_flashing() -> bool {
        false
    }

    /// An ELM327 can read live data and identify the ECU.
    pub fn supports_live_data() -> bool {
        true
    }

    /// Clamp a requested timeout into the adapter's accepted range.
    pub fn clamp_timeout_ms(requested: u32) -> u32 {
        requested.clamp(1, Self::MAX_TIMEOUT_MS)
    }

    /// Decode RPM from a Mode 01 PID 0C reply.
    ///
    /// SAE J1979: `RPM = ((A * 256) + B) / 4`. Returns `None` when the reply is
    /// not for PID 0x0C, so a mismatched frame cannot be silently misread.
    pub fn decode_rpm(r: &ElmResponse) -> Option<f64> {
        let b = pid_payload(r, 0x0C)?;
        if b.len() < 2 {
            return None;
        }
        Some((((b[0] as f64) * 256.0) + (b[1] as f64)) / 4.0)
    }

    /// Decode coolant temperature from a Mode 01 PID 05 reply.
    ///
    /// SAE J1979: `coolant_C = A - 40`.
    pub fn decode_coolant_c(r: &ElmResponse) -> Option<f64> {
        let b = pid_payload(r, 0x05)?;
        Some(b[0] as f64 - 40.0)
    }

    /// Decode throttle position from a Mode 01 PID 11 reply.
    ///
    /// SAE J1979: `throttle_pct = A * 100 / 255`.
    pub fn decode_throttle_pct(r: &ElmResponse) -> Option<f64> {
        let b = pid_payload(r, 0x11)?;
        Some((b[0] as f64) * 100.0 / 255.0)
    }
}

/// Extract the data bytes for `pid` from a Mode 01 reply.
///
/// A reply is `41 <pid> <data...>`. Returns `None` when the mode byte is not
/// `0x41` or the PID does not match, which is what keeps a mismatched frame from
/// being decoded as the wrong quantity.
fn pid_payload(r: &ElmResponse, pid: u8) -> Option<Vec<u8>> {
    if r.bytes.len() < 3 {
        return None;
    }
    if r.bytes[0] != 0x41 || r.bytes[1] != pid {
        return None;
    }
    Some(r.bytes[2..].to_vec())
}
