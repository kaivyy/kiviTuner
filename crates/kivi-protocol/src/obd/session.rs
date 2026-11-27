use crate::obd::adapter::parse_adapter_response;
use crate::obd::bitmap::SupportedPidsBitmap;
use crate::obd::classification::{classify_command, CommandClassification};
use crate::obd::command::{AdapterCommand, Mode01Pid, Mode09Item, ObdCommand};
use crate::obd::errors::ObdCodecError;
use crate::obd::framing::{ProtocolSelection, SerialBaudRate, DEFAULT_MAX_TEXT_RESPONSE_BYTES};
use crate::obd::metadata::{decode_calibration_id, decode_vin};
use crate::obd::pid::{decode_mode01_payload, DecodedMode01Value};
use crate::obd::transport::ObdTransport;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Explicit connection and transaction lifecycle states for an OBD-II session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObdSessionState {
    /// Transport is closed; no communication is active.
    Disconnected,
    /// Transport is in the process of opening hardware connection.
    Opening,
    /// Connection is open; adapter configuration handshake is underway.
    Initializing,
    /// Adapter handshake completed; session is ready for read-only diagnostic requests.
    Ready,
    /// A single transaction is actively owning the transport channel.
    TransactionInProgress,
    /// Attempting to drain trailing bytes or recover transport framing.
    Recovering,
    /// Communication failure, timeout, or desynchronization occurred; session is closed to transactions.
    Faulted { reason: String },
    /// Session is gracefully tearing down.
    Closing,
}

/// Operational parameters configuring timeouts, protocol, baud rate, and buffer limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObdSessionConfig {
    /// Maximum time allowed for an individual request-response transaction.
    pub command_timeout: Duration,
    /// Maximum bytes accepted in an adapter ASCII response buffer (default: 16 KiB).
    pub max_response_bytes: usize,
    /// Explicit protocol selection policy (e.g. Can11Bit500k via ATSP6 or Auto via ATSP0).
    pub protocol: ProtocolSelection,
    /// Target physical serial communication baud rate (default: 115,200 baud).
    pub baud_rate: SerialBaudRate,
    /// Whether to attempt querying STN-specific device information.
    pub query_stn_metadata: bool,
}

impl Default for ObdSessionConfig {
    fn default() -> Self {
        Self {
            command_timeout: Duration::from_millis(2000),
            max_response_bytes: DEFAULT_MAX_TEXT_RESPONSE_BYTES,
            protocol: ProtocolSelection::Can11Bit500k,
            baud_rate: SerialBaudRate::Baud115200,
            query_stn_metadata: true,
        }
    }
}

/// Hardware and firmware identity discovered during adapter handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ObdAdapterIdentity {
    pub elm_version: String,
    pub stn_device_id: Option<String>,
    pub stn_firmware_version: Option<String>,
    pub stn_serial_number: Option<String>,
}

/// RAII transaction guard that guarantees session state transitions to `Faulted`
/// and clears `in_transaction` if a transaction future is dropped or cancelled mid-execution.
struct TransactionGuard<'a> {
    state: &'a mut ObdSessionState,
    in_transaction: &'a mut bool,
    completed: bool,
}

impl<'a> TransactionGuard<'a> {
    fn new(state: &'a mut ObdSessionState, in_transaction: &'a mut bool) -> Self {
        *in_transaction = true;
        *state = ObdSessionState::TransactionInProgress;
        Self {
            state,
            in_transaction,
            completed: false,
        }
    }

    fn complete(mut self, target_state: ObdSessionState) {
        *self.state = target_state;
        *self.in_transaction = false;
        self.completed = true;
    }

    fn fault(mut self, reason: String) {
        *self.state = ObdSessionState::Faulted { reason };
        *self.in_transaction = false;
        self.completed = true;
    }
}

impl<'a> Drop for TransactionGuard<'a> {
    fn drop(&mut self) {
        if !self.completed {
            *self.in_transaction = false;
            *self.state = ObdSessionState::Faulted {
                reason: "transaction cancelled or dropped before completion".to_string(),
            };
        }
    }
}

/// RAII guard that transitions session state to `Faulted` if `open()` is cancelled or dropped mid-execution.
struct OpenGuard<'a> {
    state: &'a mut ObdSessionState,
    completed: bool,
}

impl<'a> OpenGuard<'a> {
    fn new(state: &'a mut ObdSessionState) -> Self {
        *state = ObdSessionState::Opening;
        Self {
            state,
            completed: false,
        }
    }

    fn complete(mut self, target_state: ObdSessionState) {
        *self.state = target_state;
        self.completed = true;
    }

    fn fault(mut self, reason: String) {
        *self.state = ObdSessionState::Faulted { reason };
        self.completed = true;
    }
}

impl<'a> Drop for OpenGuard<'a> {
    fn drop(&mut self) {
        if !self.completed {
            *self.state = ObdSessionState::Faulted {
                reason: "session open cancelled or dropped before completion".to_string(),
            };
        }
    }
}

/// RAII guard that transitions session state to `Faulted` if `close()` fails, is cancelled,
/// or is dropped mid-execution.
struct CloseGuard<'a> {
    state: &'a mut ObdSessionState,
    in_transaction: &'a mut bool,
    completed: bool,
}

impl<'a> CloseGuard<'a> {
    fn new(state: &'a mut ObdSessionState, in_transaction: &'a mut bool) -> Self {
        *state = ObdSessionState::Closing;
        *in_transaction = false;
        Self {
            state,
            in_transaction,
            completed: false,
        }
    }

    fn complete(mut self) {
        *self.state = ObdSessionState::Disconnected;
        *self.in_transaction = false;
        self.completed = true;
    }

    fn fault(mut self, reason: String) {
        *self.state = ObdSessionState::Faulted { reason };
        *self.in_transaction = false;
        self.completed = true;
    }
}

impl<'a> Drop for CloseGuard<'a> {
    fn drop(&mut self) {
        if !self.completed {
            *self.in_transaction = false;
            *self.state = ObdSessionState::Faulted {
                reason: "session close cancelled or dropped before completion".to_string(),
            };
        }
    }
}

/// Validates that an adapter configuration command was explicitly acknowledged with "OK".
///
/// Rejects:
/// - Responses containing error tokens (`?`, `CAN ERROR`, `BUS BUSY`, `FB ERROR`, `BUFFER FULL`, etc.).
/// - Responses where "OK" is only a substring within an error or other text (e.g. "LOOKS_OK_BUT_ERROR ?").
/// - Responses lacking a standalone "OK" line (case-insensitive).
fn validate_config_ack(cmd_name: &str, raw: &str) -> Result<(), ObdCodecError> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut has_ok = false;
    let mut error_line: Option<String> = None;

    for line in normalized.lines() {
        let trimmed = line.trim().trim_end_matches('>').trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("SEARCHING...") {
            continue;
        }

        let upper = trimmed.to_ascii_uppercase();
        if upper == "OK" {
            has_ok = true;
        } else if upper == "?"
            || upper.contains('?')
            || upper.contains("CAN ERROR")
            || upper.contains("BUS BUSY")
            || upper.contains("FB ERROR")
            || upper.contains("DATA ERROR")
            || upper.contains("BUFFER FULL")
            || upper.contains("UNABLE TO CONNECT")
            || upper.contains("STOPPED")
            || upper.starts_with("ERR")
        {
            error_line = Some(trimmed.to_string());
        }
    }

    if let Some(err) = error_line {
        return Err(ObdCodecError::AdapterError(format!(
            "adapter rejected {} with error: {}",
            cmd_name, err
        )));
    }

    if !has_ok {
        return Err(ObdCodecError::AdapterError(format!(
            "adapter failed to acknowledge {} with OK: received {:?}",
            cmd_name, raw
        )));
    }

    Ok(())
}

/// Validates whether a segment within a dotted version string is valid.
///
/// If `allow_single_letter_suffix` is true (e.g. for the terminal segment of "1.4b" or "1.3a"),
/// the segment may end with at most one ASCII alphabetic character provided all preceding characters are digits.
fn is_valid_version_segment(seg: &str, allow_single_letter_suffix: bool) -> bool {
    if seg.is_empty() {
        return false;
    }
    let bytes = seg.as_bytes();
    if !bytes[0].is_ascii_digit() {
        return false;
    }

    if allow_single_letter_suffix && bytes.len() > 1 && bytes[bytes.len() - 1].is_ascii_alphabetic()
    {
        return seg[..seg.len() - 1].chars().all(|c| c.is_ascii_digit());
    }

    seg.chars().all(|c| c.is_ascii_digit())
}

/// Validates whether a token represents a legitimate adapter firmware/software version string
/// (e.g. "v1.5", "v1.4b", "v1.3a", "v2.1", "v3.3.0", "v4.2.1", or "4.2.1").
///
/// Rejects malformed structures:
/// - Multiple consecutive letters (e.g. "v1abc", "v1.4abc").
/// - Consecutive or trailing dots (e.g. "v1.5..", "v1.5.", "1..5").
/// - Leading dots (e.g. ".1.5").
/// - Missing major or minor versions (e.g. "v", "v1", "v..").
/// - Suffixes not on the terminal segment (e.g. "1.4b.1").
fn is_valid_version_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }

    let raw = if token.starts_with('v') || token.starts_with('V') {
        &token[1..]
    } else {
        token
    };

    if raw.is_empty() || raw.starts_with('.') || raw.ends_with('.') || raw.contains("..") {
        return false;
    }

    let segments: Vec<&str> = raw.split('.').collect();
    // Supported versions have 2 segments (major.minor, e.g. "1.5", "1.4b")
    // or 3 segments (major.minor.patch, e.g. "3.3.0", "4.2.1")
    if segments.len() < 2 || segments.len() > 3 {
        return false;
    }

    // Segment 0 (major version) must be strictly digits
    if !is_valid_version_segment(segments[0], false) {
        return false;
    }

    if segments.len() == 2 {
        // Minor version in 2-segment version allows a single letter suffix (e.g. "1.4b", "1.3a")
        if !is_valid_version_segment(segments[1], true) {
            return false;
        }
    } else {
        // In 3-segment version (e.g. "3.3.0"), segment 1 must be digits, segment 2 (patch) may have a single letter suffix
        if !is_valid_version_segment(segments[1], false) {
            return false;
        }
        if !is_valid_version_segment(segments[2], true) {
            return false;
        }
    }

    true
}

/// Validates whether a single line conforms to the expected startup or identity banner grammar
/// for supported OBD-II adapter profiles:
/// - ELM327 (e.g. "ELM327 v1.5", "ELM327 v1.4b", "ELM 327 v1.5", "ELM v1.5")
/// - STN11xx / STN21xx (e.g. "STN1130 v3.3.0", "STN1170 v3.3.0", "STN2120 v4.2.1")
/// - OBDLink (e.g. "OBDLink SX v4.2.1", "OBDLink EX v4.2.1", "OBDLink v4.2.1")
///
/// Strictly rejects malformed lookalikes:
/// - "ELM_CORRUPTED"
/// - "ELM327_INVALID"
/// - "UNSUPPORTED STN11 DEVICE"
/// - "NOT AN OBDLINK ADAPTER"
/// - "OBDLINK_FAKE"
/// - Diagnostic payloads (e.g. "41 0C 1A F8")
/// - Standalone "OK", "?", or error strings.
pub fn is_recognized_adapter_banner(line: &str) -> bool {
    let trimmed = line.trim().trim_end_matches('>').trim();
    if trimmed.is_empty() {
        return false;
    }

    // Quick rejection of known error indicators
    let upper_full = trimmed.to_ascii_uppercase();
    if upper_full.contains('?')
        || upper_full.contains("ERROR")
        || upper_full.contains("BUS BUSY")
        || upper_full.contains("FB ERROR")
        || upper_full.contains("DATA ERROR")
        || upper_full.contains("BUFFER FULL")
        || upper_full.contains("UNABLE TO CONNECT")
        || upper_full.contains("STOPPED")
        || upper_full.starts_with("ERR")
    {
        return false;
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() {
        return false;
    }

    let token0 = tokens[0].to_ascii_uppercase();

    // 1. ELM profile: "ELM327 v1.5", "ELM 327 v1.5", "ELM v1.5"
    if token0 == "ELM327" {
        return tokens.len() >= 2 && is_valid_version_token(tokens[1]);
    } else if token0 == "ELM" {
        if tokens.len() >= 3 && tokens[1] == "327" {
            return is_valid_version_token(tokens[2]);
        } else if tokens.len() >= 2 {
            return is_valid_version_token(tokens[1]);
        }
        return false;
    }

    // 2. STN profile: Token 0 must be "STN" followed strictly by digits (e.g. "STN1130", "STN1170", "STN2120")
    if token0.starts_with("STN") && token0.len() >= 4 {
        let suffix = &token0[3..];
        if suffix.chars().all(|c| c.is_ascii_digit()) {
            return tokens.len() >= 2 && is_valid_version_token(tokens[1]);
        }
    }

    // 3. OBDLink profile: Token 0 must be "OBDLINK"
    if token0 == "OBDLINK" && tokens.len() >= 2 {
        let token1 = tokens[1].to_ascii_uppercase();
        let is_model = matches!(token1.as_str(), "SX" | "EX" | "MX" | "LX" | "CX" | "MX+");
        if is_model {
            return tokens.len() >= 3 && is_valid_version_token(tokens[2]);
        } else if is_valid_version_token(tokens[1]) {
            return true;
        }
    }

    false
}

/// Validates that an adapter reset command (ATZ) succeeded and returned a recognized startup banner
/// for the supported adapter profile (ELM327 / STN11xx / OBDLink).
///
/// Rejects:
/// - Empty or whitespace-only responses.
/// - Known adapter error tokens (`?`, `CAN ERROR`, `BUS BUSY`, `FB ERROR`, `BUFFER FULL`, etc.).
/// - Echo-only responses lacking a startup banner (e.g. just `ATZ`).
/// - Arbitrary or malformed text lacking a recognized adapter identifier.
///
/// Note on Adapter Support:
/// The supported adapter contract covers OBDLink SX (STN1130) and standard ELM327 v1.3/v1.4/v1.5/v2.1
/// chipsets. Supported startup banners identify with "ELM327", "ELM", "STN", or "OBDLink".
/// If an adapter emits unrecognized text, validation fails closed to prevent communicating with
/// unsupported or desynchronized hardware.
fn validate_reset_response(raw: &str) -> Result<(), ObdCodecError> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut recognized_banner = false;
    let mut non_empty_lines = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim().trim_end_matches('>').trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("SEARCHING...") {
            continue;
        }

        let upper = trimmed.to_ascii_uppercase();
        if upper == "?"
            || upper.contains('?')
            || upper.contains("CAN ERROR")
            || upper.contains("BUS BUSY")
            || upper.contains("FB ERROR")
            || upper.contains("DATA ERROR")
            || upper.contains("BUFFER FULL")
            || upper.contains("UNABLE TO CONNECT")
            || upper.contains("STOPPED")
            || upper.starts_with("ERR")
        {
            return Err(ObdCodecError::AdapterError(format!(
                "adapter reset failed with error: {}",
                trimmed
            )));
        }

        // Skip echoed "ATZ" command if character echo was active
        if upper == "ATZ" || upper == "AT Z" {
            continue;
        }

        non_empty_lines.push(trimmed);

        if is_recognized_adapter_banner(trimmed) {
            recognized_banner = true;
        } else {
            return Err(ObdCodecError::AdapterError(format!(
                "adapter reset returned unrecognized startup banner: {:?}",
                trimmed
            )));
        }
    }

    if non_empty_lines.is_empty() {
        return Err(ObdCodecError::AdapterError(
            "empty or echo-only response from adapter on reset".to_string(),
        ));
    }

    if !recognized_banner {
        return Err(ObdCodecError::AdapterError(format!(
            "adapter reset returned unrecognized startup banner: {:?}",
            raw.trim()
        )));
    }

    Ok(())
}

/// Validates and extracts the ELM version banner from QueryElmVersion (`ATI`).
///
/// Validates that the banner matches the documented identifier pattern for the supported
/// adapter family profile (ELM327 / STN11xx / OBDLink).
///
/// Rejects:
/// - Empty or whitespace-only responses.
/// - Error tokens (`?`, `CAN ERROR`, etc.).
/// - Arbitrary or unrelated text lacking a recognized adapter family identity.
///
/// Note: While this validates that the banner matches the supported adapter family profile,
/// ASCII text banners do not provide cryptographic authenticity and cannot prove that physical
/// hardware is genuine (clones can emulate banner strings).
fn validate_elm_version_response(raw: &str) -> Result<String, ObdCodecError> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut banner: Option<String> = None;

    for line in normalized.lines() {
        let trimmed = line.trim().trim_end_matches('>').trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("SEARCHING...") {
            continue;
        }

        let upper = trimmed.to_ascii_uppercase();
        if upper == "?"
            || upper.contains('?')
            || upper.contains("CAN ERROR")
            || upper.contains("BUS BUSY")
            || upper.contains("FB ERROR")
            || upper.contains("DATA ERROR")
            || upper.contains("BUFFER FULL")
            || upper.contains("UNABLE TO CONNECT")
            || upper.contains("STOPPED")
            || upper.starts_with("ERR")
        {
            return Err(ObdCodecError::AdapterError(format!(
                "adapter query ELM version failed with error: {}",
                trimmed
            )));
        }

        // Skip echoed "ATI" command if character echo was active
        if upper == "ATI" || upper == "AT I" {
            continue;
        }

        if is_recognized_adapter_banner(trimmed) {
            banner = Some(trimmed.to_string());
        } else {
            return Err(ObdCodecError::AdapterError(format!(
                "adapter reported unrecognized version banner: {}",
                trimmed
            )));
        }
    }

    banner.ok_or_else(|| {
        ObdCodecError::AdapterError("empty response from adapter on version query".to_string())
    })
}

/// Coordinates exclusive session ownership, typed transactions, and fault isolation
/// over an underlying `ObdTransport`.
pub struct ObdSession<T: ObdTransport> {
    transport: T,
    state: ObdSessionState,
    config: ObdSessionConfig,
    identity: Option<ObdAdapterIdentity>,
    in_transaction: bool,
}

impl<T: ObdTransport> ObdSession<T> {
    /// Creates a new disconnected OBD session with the provided transport and config.
    pub fn new(transport: T, config: ObdSessionConfig) -> Self {
        Self {
            transport,
            state: ObdSessionState::Disconnected,
            config,
            identity: None,
            in_transaction: false,
        }
    }

    /// Returns the current lifecycle state of the session.
    pub fn state(&self) -> &ObdSessionState {
        &self.state
    }

    /// Returns the adapter identity discovered during initialization, if any.
    pub fn identity(&self) -> Option<&ObdAdapterIdentity> {
        self.identity.as_ref()
    }

    /// Opens the underlying transport channel and transitions to `Initializing`.
    pub async fn open(&mut self) -> Result<(), ObdCodecError> {
        if self.state != ObdSessionState::Disconnected {
            return Err(ObdCodecError::InvalidSessionState {
                current: format!("{:?}", self.state),
                expected: "Disconnected".to_string(),
            });
        }

        let guard = OpenGuard::new(&mut self.state);
        if let Err(e) = self.transport.open().await {
            guard.fault(format!("failed to open transport: {}", e));
            return Err(e);
        }

        guard.complete(ObdSessionState::Initializing);
        Ok(())
    }

    /// Executes the evidence-driven adapter initialization handshake.
    ///
    /// Sends step-by-step allowlisted configuration commands and queries identity.
    /// Transitions session to `Ready` on success, or `Faulted` on fatal error.
    pub async fn initialize(&mut self) -> Result<ObdAdapterIdentity, ObdCodecError> {
        if self.state != ObdSessionState::Initializing {
            return Err(ObdCodecError::InvalidSessionState {
                current: format!("{:?}", self.state),
                expected: "Initializing".to_string(),
            });
        }

        // 1. Reset adapter
        let reset_raw = match self
            .transact_raw(ObdCommand::Adapter(AdapterCommand::Reset))
            .await
        {
            Ok(raw) => raw,
            Err(e) => {
                self.state = ObdSessionState::Faulted {
                    reason: format!("reset failed: {}", e),
                };
                return Err(e);
            }
        };
        if let Err(e) = validate_reset_response(&reset_raw) {
            self.state = ObdSessionState::Faulted {
                reason: e.to_string(),
            };
            return Err(e);
        }

        // 2. Disable character echo (ATE0)
        let echo_raw = match self
            .transact_raw(ObdCommand::Adapter(AdapterCommand::EchoOff))
            .await
        {
            Ok(raw) => raw,
            Err(e) => {
                self.state = ObdSessionState::Faulted {
                    reason: format!("echo off failed: {}", e),
                };
                return Err(e);
            }
        };
        if let Err(e) = validate_config_ack("ATE0", &echo_raw) {
            self.state = ObdSessionState::Faulted {
                reason: e.to_string(),
            };
            return Err(e);
        }

        // 3. Disable linefeeds (ATL0)
        let lf_raw = match self
            .transact_raw(ObdCommand::Adapter(AdapterCommand::LinefeedsOff))
            .await
        {
            Ok(raw) => raw,
            Err(e) => {
                self.state = ObdSessionState::Faulted {
                    reason: format!("linefeeds off failed: {}", e),
                };
                return Err(e);
            }
        };
        if let Err(e) = validate_config_ack("ATL0", &lf_raw) {
            self.state = ObdSessionState::Faulted {
                reason: e.to_string(),
            };
            return Err(e);
        }

        // 4. Disable CAN headers (ATH0)
        let h_raw = match self
            .transact_raw(ObdCommand::Adapter(AdapterCommand::HeadersOff))
            .await
        {
            Ok(raw) => raw,
            Err(e) => {
                self.state = ObdSessionState::Faulted {
                    reason: format!("headers off failed: {}", e),
                };
                return Err(e);
            }
        };
        if let Err(e) = validate_config_ack("ATH0", &h_raw) {
            self.state = ObdSessionState::Faulted {
                reason: e.to_string(),
            };
            return Err(e);
        }

        // 5. Enable space delimiters (ATS1)
        let s_raw = match self
            .transact_raw(ObdCommand::Adapter(AdapterCommand::SpacesOn))
            .await
        {
            Ok(raw) => raw,
            Err(e) => {
                self.state = ObdSessionState::Faulted {
                    reason: format!("spaces on failed: {}", e),
                };
                return Err(e);
            }
        };
        if let Err(e) = validate_config_ack("ATS1", &s_raw) {
            self.state = ObdSessionState::Faulted {
                reason: e.to_string(),
            };
            return Err(e);
        }

        // 6. Target protocol profile selection (ATSP6 or ATSP0)
        let proto_cmd = self.config.protocol.to_adapter_command();
        let proto_raw = match self.transact_raw(ObdCommand::Adapter(proto_cmd)).await {
            Ok(raw) => raw,
            Err(e) => {
                self.state = ObdSessionState::Faulted {
                    reason: format!("protocol selection failed: {}", e),
                };
                return Err(e);
            }
        };
        if let Err(e) = validate_config_ack(&format!("{:?}", proto_cmd), &proto_raw) {
            self.state = ObdSessionState::Faulted {
                reason: e.to_string(),
            };
            return Err(e);
        }

        // 7. Query ELM version
        let elm_raw = match self
            .transact_raw(ObdCommand::Adapter(AdapterCommand::QueryElmVersion))
            .await
        {
            Ok(raw) => raw,
            Err(e) => {
                self.state = ObdSessionState::Faulted {
                    reason: format!("query ELM version failed: {}", e),
                };
                return Err(e);
            }
        };
        let elm_version = match validate_elm_version_response(&elm_raw) {
            Ok(v) => v,
            Err(e) => {
                self.state = ObdSessionState::Faulted {
                    reason: e.to_string(),
                };
                return Err(e);
            }
        };

        let mut identity = ObdAdapterIdentity {
            elm_version,
            stn_device_id: None,
            stn_firmware_version: None,
            stn_serial_number: None,
        };

        // 8. Optional STN metadata queries (transport errors fail closed; "?" marks unsupported)
        if self.config.query_stn_metadata {
            let stdi_raw = self
                .transact_raw(ObdCommand::Adapter(AdapterCommand::QueryStnDeviceId))
                .await?;
            let stdi_clean = stdi_raw.trim().trim_end_matches('>').trim();
            if !stdi_clean.is_empty() && stdi_clean != "?" && !stdi_clean.contains("ERROR") {
                identity.stn_device_id = Some(stdi_clean.to_string());

                let sti_raw = self
                    .transact_raw(ObdCommand::Adapter(AdapterCommand::QueryStnFirmwareVersion))
                    .await?;
                let sti_clean = sti_raw.trim().trim_end_matches('>').trim();
                if !sti_clean.is_empty() && sti_clean != "?" && !sti_clean.contains("ERROR") {
                    identity.stn_firmware_version = Some(sti_clean.to_string());
                }

                let stsn_raw = self
                    .transact_raw(ObdCommand::Adapter(AdapterCommand::QueryStnSerialNumber))
                    .await?;
                let stsn_clean = stsn_raw.trim().trim_end_matches('>').trim();
                if !stsn_clean.is_empty() && stsn_clean != "?" && !stsn_clean.contains("ERROR") {
                    identity.stn_serial_number = Some(stsn_clean.to_string());
                }
            }
        }

        self.identity = Some(identity.clone());
        self.state = ObdSessionState::Ready;
        Ok(identity)
    }

    /// Internal helper executing a single raw ASCII transaction during handshake or diagnostics.
    async fn transact_raw(&mut self, cmd: ObdCommand) -> Result<String, ObdCodecError> {
        if self.in_transaction {
            return Err(ObdCodecError::SessionBusy);
        }

        match &self.state {
            ObdSessionState::Faulted { reason } => {
                return Err(ObdCodecError::SessionFaulted(reason.clone()));
            }
            ObdSessionState::Disconnected | ObdSessionState::Closing => {
                return Err(ObdCodecError::InvalidSessionState {
                    current: format!("{:?}", self.state),
                    expected: "Initializing or Ready".to_string(),
                });
            }
            ObdSessionState::Opening => {
                return Err(ObdCodecError::InvalidSessionState {
                    current: "Opening".to_string(),
                    expected: "Initializing or Ready".to_string(),
                });
            }
            ObdSessionState::Initializing => {
                // In Initializing, only adapter configuration/identity/reset is allowed
                if classify_command(&cmd) == CommandClassification::EcuDiagnosticRequest {
                    return Err(ObdCodecError::UnauthorizedCommand(
                        "ECU diagnostic requests not permitted before session is Ready".to_string(),
                    ));
                }
            }
            ObdSessionState::Ready => {
                // In Ready, adapter reset/configuration is forbidden
                let classification = classify_command(&cmd);
                if classification == CommandClassification::AdapterReset
                    || classification == CommandClassification::AdapterConfiguration
                {
                    return Err(ObdCodecError::UnauthorizedCommand(
                        "Adapter configuration mutation not permitted in Ready state".to_string(),
                    ));
                }
            }
            ObdSessionState::TransactionInProgress | ObdSessionState::Recovering => {
                return Err(ObdCodecError::SessionBusy);
            }
        }

        let prev_state = self.state.clone();
        let guard = TransactionGuard::new(&mut self.state, &mut self.in_transaction);

        // 1. Write typed command
        // Note on Partial Physical Writes:
        // `write_command` returns `Result<(), ObdCodecError>`. The current transport interface does not
        // report partial byte offsets written to physical hardware buffers before an error or cancellation.
        // If a write fails or is cancelled, the session immediately moves to `Faulted` and rejects all
        // subsequent commands, preventing cross-talk or desynchronization.
        if let Err(e) = self.transport.write_command(cmd).await {
            guard.fault(format!("write command failed: {}", e));
            return Err(e);
        }

        // 2. Read response up to deadline
        let response = match self
            .transport
            .read_response(self.config.command_timeout, self.config.max_response_bytes)
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                // Timeout or I/O failure corrupts transaction framing; transition to Faulted
                guard.fault(format!("read response failed or timed out: {}", e));
                return Err(e);
            }
        };

        guard.complete(prev_state);
        Ok(response)
    }

    /// Executes an approved typed OBD command and decodes the response into diagnostic bytes.
    pub async fn execute_command(&mut self, cmd: ObdCommand) -> Result<Vec<u8>, ObdCodecError> {
        let raw_text = self.transact_raw(cmd).await?;
        parse_adapter_response(&raw_text)
    }

    /// Queries a Mode 01 PID and returns the strongly-typed decoded value.
    pub async fn query_pid(&mut self, pid: Mode01Pid) -> Result<DecodedMode01Value, ObdCodecError> {
        let payload = self.execute_command(ObdCommand::Mode01(pid)).await?;
        decode_mode01_payload(pid, &payload)
    }

    /// Queries a 32-PID supported bitmap range and returns the parsed bitmap.
    pub async fn query_supported_pids(
        &mut self,
        base: Mode01Pid,
    ) -> Result<SupportedPidsBitmap, ObdCodecError> {
        let payload = self.execute_command(ObdCommand::Mode01(base)).await?;
        SupportedPidsBitmap::decode_from_payload(&payload)
    }

    /// Queries the Vehicle Identification Number (VIN) via Mode 09 PID 0x02.
    pub async fn query_vin(&mut self) -> Result<String, ObdCodecError> {
        let payload = self
            .execute_command(ObdCommand::Mode09(Mode09Item::Vin))
            .await?;
        decode_vin(&payload)
    }

    /// Queries the ECU Calibration ID via Mode 09 PID 0x04.
    pub async fn query_calibration_id(&mut self) -> Result<String, ObdCodecError> {
        let payload = self
            .execute_command(ObdCommand::Mode09(Mode09Item::CalibrationId))
            .await?;
        decode_calibration_id(&payload)
    }

    /// Gracefully closes the session and releases transport resources.
    ///
    /// # Concurrency & Ownership Model
    /// `close(&mut self)` requires an exclusive mutable borrow of `ObdSession`. Under safe Rust
    /// semantics, a caller cannot invoke `close()` concurrently with any active transaction method
    /// (`execute_command`, `query_pid`, etc.) on the same session instance.
    ///
    /// When shared across asynchronous tasks (e.g. wrapped in `Arc<tokio::sync::Mutex<ObdSession>>`),
    /// the mutex serializes access: `close()` can only execute when no other task holds the session lock.
    /// If an in-flight transaction task is cancelled or dropped, `TransactionGuard::drop` runs
    /// synchronously first, releasing `in_transaction` and moving the state to `Faulted`, after
    /// which `close()` can safely teardown and disconnect the transport.
    pub async fn close(&mut self) -> Result<(), ObdCodecError> {
        let guard = CloseGuard::new(&mut self.state, &mut self.in_transaction);
        if let Err(e) = self.transport.close().await {
            guard.fault(format!("transport close failed: {}", e));
            return Err(e);
        }

        guard.complete();
        Ok(())
    }
}
