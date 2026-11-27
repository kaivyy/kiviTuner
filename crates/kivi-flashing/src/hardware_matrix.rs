//! Honest hardware verification matrix for supported transports and flashing protocols.
//!
//! Every target starts cleanly classified with explicit evidence status so
//! software never misrepresents virtual CI coverage as real ECU bench sign-off.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HardwareVerificationTier {
    /// Verified end-to-end against physical ECU hardware with captured traces.
    BenchVerified,
    /// Read/diagnostic identification tested against physical hardware; write remains locked.
    ReadOnlyVerified,
    /// Protocol framing and virtual ECU tests pass in CI; physical ECU capture pending.
    ExperimentalUntested,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareTargetCapability {
    pub id: String,
    pub name: String,
    pub category: String,
    pub tier: HardwareVerificationTier,
    pub supports_read: bool,
    pub supports_write: bool,
    pub requires_write_unlock: bool,
    pub evidence_note: String,
}

/// Returns true if live hardware writes are locked by default for `tier`.
pub fn is_tier_write_locked_by_default(tier: HardwareVerificationTier) -> bool {
    !matches!(tier, HardwareVerificationTier::BenchVerified)
}

/// Returns the honest hardware verification catalog for all shipped transports & protocols.
pub fn hardware_verification_matrix() -> Vec<HardwareTargetCapability> {
    vec![
        HardwareTargetCapability {
            id: "transport_openport2".into(),
            name: "Tactrix OpenPort 2.0 (USB-CDC)".into(),
            category: "Transport".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "Serial CDC framing tested against virtual doubles; awaiting real vehicle USB trace.".into(),
        },
        HardwareTargetCapability {
            id: "transport_j2534".into(),
            name: "SAE J2534 Pass-Thru DLL/SO (Win32 x86, Win64 x64 + 32-bit Bridge)".into(),
            category: "Transport".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "Win32 stdcall & Win64 FFI struct layout + out-of-process 32-bit bridge IPC verified; awaiting physical vendor DLL run.".into(),
        },
        HardwareTargetCapability {
            id: "transport_socketcan".into(),
            name: "Linux SocketCAN (AF_CAN)".into(),
            category: "Transport".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "CAN_RAW single-frame classic CAN implemented; ISO 15765-2 segmented transfers require J2534 or OpenPort 2.0; awaiting physical CAN capture.".into(),
        },
        HardwareTargetCapability {
            id: "transport_doip".into(),
            name: "ISO 13400 DoIP Ethernet".into(),
            category: "Transport".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "Routing activation and DoIP header codec unit-tested; awaiting DoIP gateway capture.".into(),
        },
        HardwareTargetCapability {
            id: "transport_elm_serial".into(),
            name: "ELM327 / STN1110 Serial OBD".into(),
            category: "Transport".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: false,
            requires_write_unlock: true,
            evidence_note: "AT command state machine & Mode 01/03/09 decoder tested; slow-adapter write disabled.".into(),
        },
        HardwareTargetCapability {
            id: "transport_ostrich2".into(),
            name: "Ostrich 2.0 Real-Time EPROM Emulator".into(),
            category: "Transport".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: false,
            evidence_note: "256-byte page framing & checksum verified against mock SRAM; awaiting physical hardware probe.".into(),
        },
        HardwareTargetCapability {
            id: "proto_uds_iso14229".into(),
            name: "UDS ISO 14229 (CAN / DoIP)".into(),
            category: "Protocol".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "Full virtual ECU dump/erase/flash/NRC recovery lifecycle verified in E2E suite.".into(),
        },
        HardwareTargetCapability {
            id: "proto_gm_delco".into(),
            name: "GM Delco VPW / GMLAN (P01/P59/E38/E92)".into(),
            category: "Protocol".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "256-algo Seed/Key tables & Class-2 VPW framing tested; awaiting physical bench harness.".into(),
        },
        HardwareTargetCapability {
            id: "proto_denso_sh705x".into(),
            name: "Denso SuperH SH705x (SSM CAN / K-Line)".into(),
            category: "Protocol".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "Feistel security & RAM kernel transfer tested against mocks; awaiting bench ECU.".into(),
        },
        HardwareTargetCapability {
            id: "proto_honda_moto".into(),
            name: "Honda Keihin / Shindengen Moto K-Line".into(),
            category: "Protocol".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "Write boundaries, ROM key extraction & K-Line framing tested; awaiting physical bike ECU.".into(),
        },
        HardwareTargetCapability {
            id: "proto_vag_sa2".into(),
            name: "VAG SA2 / Bosch ME7 / Simos Container".into(),
            category: "Protocol".into(),
            tier: HardwareVerificationTier::ExperimentalUntested,
            supports_read: true,
            supports_write: true,
            requires_write_unlock: true,
            evidence_note: "SA2 bytecode VM & ODX/FRF/SGO codecs tested; awaiting physical ECU bench capture.".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_entries_are_complete_and_honest() {
        let items = hardware_verification_matrix();
        assert!(items.len() >= 10);
        for item in &items {
            assert!(!item.id.is_empty());
            assert!(!item.name.is_empty());
            assert!(!item.evidence_note.is_empty());
            assert_ne!(
                item.tier,
                HardwareVerificationTier::BenchVerified,
                "do not mark {} BenchVerified without real physical capture",
                item.id
            );
        }
        assert!(is_tier_write_locked_by_default(
            HardwareVerificationTier::ExperimentalUntested
        ));
        assert!(is_tier_write_locked_by_default(
            HardwareVerificationTier::ReadOnlyVerified
        ));
        assert!(!is_tier_write_locked_by_default(
            HardwareVerificationTier::BenchVerified
        ));
    }
}
