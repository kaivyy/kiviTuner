//! Ported from `ecu_engine_rust::flashing::module_registry`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Target Electronic Control Unit Category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetUnitType {
    EngineEcu,
    TransmissionTcu,
    HybridInverterOrCpc,
}

/// Primary Communication & Boot Transport Bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashTransportBus {
    KLineIso9141OrKwp,
    CanIsoTp11BitOr29Bit,
    CanFdIsoTp,
    VagTp20,
    EthernetDoIp,
    J1850PwmOrVpw,
    BenchServiceModeGpt,
    BootPinBslOrAud,
}

/// Action Capability Flags (`[RD]`, `[VR]`, `[WR]`, `[CK]`, `[CLONE]`, `[SERVICE_MODE]`, `[VID]`, `[TCU_REBOOT]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCapabilities {
    pub direct_read: bool,        // [RD]
    pub virtual_read: bool,       // [VR]
    pub write_flash: bool,        // [WR]
    pub checksum_correct: bool,   // [CK]
    pub full_clone_eeprom: bool,  // [CLONE] Flash + EEPROM
    pub cover_free_service: bool, // [SM] GPT1/GPT2 Service Mode without opening ECU
    pub auto_vid_transfer: bool,  // Ford/Mazda VID block auto-transfer
    pub tcu_clone_reboot: bool,   // Dedicated TCU Reboot command
}

/// Multi-vendor Flash Module Descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashModuleDescriptor {
    pub module_id: u16,
    pub name: &'static str,
    pub mcu_family: &'static str,
    pub unit_type: TargetUnitType,
    pub transport: FlashTransportBus,
    pub flash_size_bytes: usize,
    pub eeprom_size_bytes: usize,
    pub capabilities: ModuleCapabilities,
}

/// Requested Operation on a Flash Module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashOperationRequest {
    DirectRead,
    VirtualRead,
    WriteCalibrationOrFlash,
    CloneFullBackup,
    ServiceModePasswordRead,
    TcuReboot,
}

/// Unified Flash Module Dispatcher & Registry.
pub struct UnifiedFlashModuleRegistry {
    modules: Vec<FlashModuleDescriptor>,
}

impl Default for UnifiedFlashModuleRegistry {
    fn default() -> Self {
        Self::new_standard_catalog()
    }
}

impl UnifiedFlashModuleRegistry {
    /// Construct the standard multi-vendor catalog covering powertrain, transmission, and hybrid modules.
    pub fn new_standard_catalog() -> Self {
        let modules = vec![
            FlashModuleDescriptor {
                module_id: 1,
                name: "Ford Focus 3 / Fiesta / EcoSport (EMS2204/SID208/MED17)",
                mcu_family: "Infineon TC1767 / TC1797",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::CanIsoTp11BitOr29Bit,
                flash_size_bytes: 2048 * 1024,
                eeprom_size_bytes: 64 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: true,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: false,
                    cover_free_service: false,
                    auto_vid_transfer: true,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 15,
                name: "Nissan / Infiniti K-Line & CAN (Hitachi SH7055/SH7058/SH7059)",
                mcu_family: "Renesas SH7055 / SH7058 / SH7059",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::CanIsoTp11BitOr29Bit,
                flash_size_bytes: 1024 * 1024,
                eeprom_size_bytes: 4 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: false,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: false,
                    cover_free_service: false,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 24,
                name: "Mitsubishi CAN-bus (MH8104F/MH8106F/MH8115F/MH8601/MH8611)",
                mcu_family: "Mitsubishi Melco MH810x / MH8115 / MH86xx",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::CanIsoTp11BitOr29Bit,
                flash_size_bytes: 1280 * 1024,
                eeprom_size_bytes: 0,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: false,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: true,
                    cover_free_service: false,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 35,
                name: "Toyota / Lexus / Scion / Hino Gen1 & Gen2 (76F00xx / Denso)",
                mcu_family: "Fujitsu 76F0038 / 76F0040 / 76F0070 / 76F0085",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::CanIsoTp11BitOr29Bit,
                flash_size_bytes: 768 * 1024,
                eeprom_size_bytes: 0,
                capabilities: ModuleCapabilities {
                    direct_read: false, // Toyota OBD blocks direct read -> requires Virtual Read [VR]
                    virtual_read: true,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: false,
                    cover_free_service: false,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 37,
                name: "Honda / Acura CAN-bus & Moto Keihin (PGM-FI SH7058 / SH72543)",
                mcu_family: "Renesas SH7058 / SH72543",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::KLineIso9141OrKwp,
                flash_size_bytes: 1024 * 1024,
                eeprom_size_bytes: 0,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: true,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: false,
                    cover_free_service: false,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 50,
                name: "VAG EDC17 / MED17 / Simos 12 / Simos 18 UDS",
                mcu_family: "Infineon TC1796 / TC1797 / TC1791",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::CanIsoTp11BitOr29Bit,
                flash_size_bytes: 4096 * 1024,
                eeprom_size_bytes: 128 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: false, // UDS OBD uses Virtual Read [VR]
                    virtual_read: true,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: false,
                    cover_free_service: false,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 53,
                name: "Infineon Tricore BSL (TC1724/TC1762/TC1766/TC1767/TC1796/TC1797/TC1791)",
                mcu_family: "Infineon TriCore BSL",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::BootPinBslOrAud,
                flash_size_bytes: 4096 * 1024,
                eeprom_size_bytes: 128 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: false,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: true,
                    cover_free_service: false,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 57,
                name: "VAG SIMOS 8 / 10 / 11 / 12 / PCR2.1 (EEPROM TPROT Unlock & Write)",
                mcu_family: "Infineon TC1796 / TC1797",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::CanIsoTp11BitOr29Bit,
                flash_size_bytes: 2048 * 1024,
                eeprom_size_bytes: 8 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: true,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: true,
                    cover_free_service: true,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 58,
                name: "VAG DSG / CVT (DQ200 / DQ250 / DQ381 / DQ500 / VL381 / DL501)",
                mcu_family: "Infineon TC1766 / TC1784 / MPC555",
                unit_type: TargetUnitType::TransmissionTcu,
                transport: FlashTransportBus::BenchServiceModeGpt,
                flash_size_bytes: 2048 * 1024,
                eeprom_size_bytes: 64 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: true,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: true,
                    cover_free_service: true,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: true,
                },
            },
            FlashModuleDescriptor {
                module_id: 71,
                name: "Bosch MEDC17 Bootloader (Cover-Free GPT1/GPT2 Service Mode)",
                mcu_family: "Infineon TC1724 / TC1728 / TC1762 / TC1766 / TC1767 / TC1782 / TC1791 / TC1793 / TC1796 / TC1797",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::BenchServiceModeGpt,
                flash_size_bytes: 4096 * 1024,
                eeprom_size_bytes: 128 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: false,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: true,
                    cover_free_service: true,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 89,
                name: "China Delphi / Bosch CAN-FD (PD05 / PD08 / MT92.1 / MG1UA008)",
                mcu_family: "Infineon Aurix TC2xx / SPC58",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::CanFdIsoTp,
                flash_size_bytes: 4096 * 1024,
                eeprom_size_bytes: 64 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: true,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: false,
                    cover_free_service: false,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 92,
                name: "Bosch MD1 / MG1 Bootloader (Aurix TC265/TC275/TC277/TC298/TC299/TC387/TC399 Service Mode)",
                mcu_family: "Infineon Aurix TC2xx / TC3xx / SPC5777",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::BenchServiceModeGpt,
                flash_size_bytes: 8192 * 1024,
                eeprom_size_bytes: 256 * 1024,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: false,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: true,
                    cover_free_service: true,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
            FlashModuleDescriptor {
                module_id: 95,
                name: "Toyota / Lexus Gen 4 P5-UDS (R7F701202 / R7F701216 / R7F702002)",
                mcu_family: "Renesas RH850 R7F701202 / R7F702002",
                unit_type: TargetUnitType::EngineEcu,
                transport: FlashTransportBus::CanFdIsoTp,
                flash_size_bytes: 4096 * 1024,
                eeprom_size_bytes: 0,
                capabilities: ModuleCapabilities {
                    direct_read: true,
                    virtual_read: true,
                    write_flash: true,
                    checksum_correct: true,
                    full_clone_eeprom: false,
                    cover_free_service: false,
                    auto_vid_transfer: false,
                    tcu_clone_reboot: false,
                },
            },
        ];

        Self { modules }
    }

    /// Look up module descriptor by numeric Module ID.
    pub fn get_module(&self, module_id: u16) -> Option<&FlashModuleDescriptor> {
        self.modules.iter().find(|m| m.module_id == module_id)
    }

    /// Search modules matching a keyword (e.g., `"MEDC17"`, `"DSG"`, `"MH8115"`, `"CAN-FD"`).
    pub fn search_modules(&self, query: &str) -> Vec<&FlashModuleDescriptor> {
        let q = query.to_uppercase();
        self.modules
            .iter()
            .filter(|m| {
                m.name.to_uppercase().contains(&q) || m.mcu_family.to_uppercase().contains(&q)
            })
            .collect()
    }

    /// Validate whether a module supports the requested operation, or suggest fallback (e.g. `VirtualRead`).
    pub fn validate_operation(
        &self,
        module_id: u16,
        op: FlashOperationRequest,
    ) -> Result<&FlashModuleDescriptor, &'static str> {
        let m = self
            .get_module(module_id)
            .ok_or("Module ID not found in registry")?;

        match op {
            FlashOperationRequest::DirectRead => {
                if m.capabilities.direct_read {
                    Ok(m)
                } else if m.capabilities.virtual_read {
                    Err("DIRECT_READ_BLOCKED_USE_VIRTUAL_READ: This ECU blocks direct OBD read; use Virtual Read [VR] or Service Mode Module 71/92")
                } else {
                    Err("Direct Read not supported on this module")
                }
            }
            FlashOperationRequest::VirtualRead => {
                if m.capabilities.virtual_read {
                    Ok(m)
                } else {
                    Err("Virtual Read not supported; use Direct Read [RD]")
                }
            }
            FlashOperationRequest::WriteCalibrationOrFlash => {
                if m.capabilities.write_flash {
                    Ok(m)
                } else {
                    Err("Write operation not supported on this module")
                }
            }
            FlashOperationRequest::CloneFullBackup => {
                if m.capabilities.full_clone_eeprom {
                    Ok(m)
                } else {
                    Err("Full EEPROM/Flash cloning requires Service Mode or BSL Module (e.g. Module 53/58/71/92)")
                }
            }
            FlashOperationRequest::ServiceModePasswordRead => {
                if m.capabilities.cover_free_service {
                    Ok(m)
                } else {
                    Err("Cover-free GPT Service Mode not supported on this module")
                }
            }
            FlashOperationRequest::TcuReboot => {
                if m.capabilities.tcu_clone_reboot {
                    Ok(m)
                } else {
                    Err("TCU Reboot is only supported on Transmission TCU modules")
                }
            }
        }
    }
}

/// Supported Transmission Control Unit (TCU / Mechatronic) Families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcuFamily {
    VagDq200G2,
    VagDq250MqB,
    VagDq381,
    VagDq500,
    VagDl501,
    Getrag6Dct450,
    Zf8Hp,
    MercedesVgsNag3,
}

/// TCU / Gearbox Mechatronic Cloning & Reboot Controller.
pub struct TcuMechatronicController;

impl TcuMechatronicController {
    /// Return default `(flash_size_bytes, eeprom_size_bytes)` for a TCU Mechatronic family.
    pub fn memory_layout(family: TcuFamily) -> (usize, usize) {
        match family {
            TcuFamily::VagDq200G2 => (1536 * 1024, 64 * 1024),
            TcuFamily::VagDq250MqB => (2048 * 1024, 64 * 1024),
            TcuFamily::VagDq381 | TcuFamily::VagDq500 => (4096 * 1024, 128 * 1024),
            TcuFamily::VagDl501 => (2048 * 1024, 64 * 1024),
            TcuFamily::Getrag6Dct450 => (1024 * 1024, 32 * 1024),
            TcuFamily::Zf8Hp => (2048 * 1024, 64 * 1024),
            TcuFamily::MercedesVgsNag3 => (2560 * 1024, 64 * 1024),
        }
    }

    /// Build specialized TCU Mechatronic Clone Reboot & Adaptation Latch UDS command.
    pub fn build_tcu_clone_reboot_sequence(family: TcuFamily) -> Vec<Vec<u8>> {
        match family {
            TcuFamily::VagDq200G2
            | TcuFamily::VagDq250MqB
            | TcuFamily::VagDq381
            | TcuFamily::VagDq500
            | TcuFamily::VagDl501 => vec![
                vec![0x31, 0x01, 0xFF, 0x01], // RoutineControl: Latch cloned EEPROM & Mechatronic adaptation
                vec![0x11, 0x01],             // ECUReset: HardReset Mechatronic TCU
            ],
            TcuFamily::Getrag6Dct450 | TcuFamily::Zf8Hp | TcuFamily::MercedesVgsNag3 => vec![
                vec![0x10, 0x03], // ExtendedDiagnosticSession
                vec![0x11, 0x03], // ECUReset: SoftReset & Re-initialize clutch pressures
            ],
        }
    }
}
