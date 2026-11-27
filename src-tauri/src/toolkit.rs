//! Wave 8: application integration for the physics / flashing / telemetry crates.
//!
//! Each command here is a thin, typed boundary over a pure library function.
//! Nothing in this file opens a device, writes to an ECU, or touches the
//! filesystem — the libraries underneath are byte-level only.
//!
//! ## Why a single `toolkit` module
//!
//! The three migrated crates expose ~490 functions. Exposing them one command at
//! a time would produce an unusable command surface. This module groups them by
//! **what a tuner actually does**: compute a fuel pulse, check a flash plan,
//! inspect a container, decode a sensor. Each command takes explicit inputs and
//! returns a typed result.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Physics: fuel pulse calculation
// ---------------------------------------------------------------------------

/// Inputs for a fuel pulse-width calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuelPulseRequest {
    /// Air mass inducted per cylinder, grams.
    pub air_mass_g: f64,
    /// Target lambda (1.0 = stoichiometric).
    pub target_lambda: f64,
    /// Battery voltage.
    pub battery_volts: f64,
    /// Rail differential pressure, kPa.
    pub rail_diff_kpa: f64,
    /// Engine cycle period, seconds (`120 / rpm` for a 4-stroke).
    pub cycle_dt_sec: f64,
    /// Fuel type: `gasoline`, `e10`, `e85`, `methanol`, `diesel`.
    pub fuel: String,
}

/// Result of a fuel pulse-width calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FuelPulseResult {
    /// Gross injector pulse width, milliseconds.
    pub gross_ms: f64,
    /// Effective (fuel-delivering) pulse width, milliseconds.
    pub effective_ms: f64,
    /// Injector dead time, milliseconds.
    pub dead_time_ms: f64,
    /// Stoichiometric AFR used for the target.
    pub stoich_afr: f64,
}

fn fuel_type_from_str(s: &str) -> Result<kivi_physics::fuel::FuelType, String> {
    use kivi_physics::fuel::FuelType;
    match s.to_ascii_lowercase().as_str() {
        "gasoline" => Ok(FuelType::PureGasoline),
        "e10" => Ok(FuelType::E10Gasoline),
        "e85" => Ok(FuelType::E85Ethanol),
        "methanol" => Ok(FuelType::PureMethanol),
        "diesel" => Ok(FuelType::Diesel),
        other => Err(format!(
            "Unknown fuel type '{}' (expected gasoline, e10, e85, methanol, diesel)",
            other
        )),
    }
}

/// Computes injector pulse width from an air charge and a lambda target.
pub fn compute_fuel_pulse_inner(req: &FuelPulseRequest) -> Result<FuelPulseResult, String> {
    if !req.air_mass_g.is_finite() || req.air_mass_g < 0.0 {
        return Err("Air mass must be a finite non-negative number".to_string());
    }
    if !req.target_lambda.is_finite() || req.target_lambda <= 0.0 {
        return Err("Target lambda must be a finite positive number".to_string());
    }
    if !req.battery_volts.is_finite() || req.battery_volts <= 0.0 {
        return Err("Battery voltage must be a finite positive number".to_string());
    }

    let fuel = fuel_type_from_str(&req.fuel)?;
    let stoich = fuel.stoichiometric_afr();
    let mut controller = kivi_physics::fuel::FuelController::new(
        fuel,
        kivi_physics::fuel::FuelInjector::default(),
    );

    let (gross, eff, dead) = controller.calculate_pulse_width_ms(
        req.air_mass_g,
        req.target_lambda,
        req.battery_volts,
        req.rail_diff_kpa,
        req.cycle_dt_sec,
    );

    if !gross.is_finite() || !eff.is_finite() || !dead.is_finite() {
        return Err("Pulse-width computation produced a non-finite value".to_string());
    }

    Ok(FuelPulseResult {
        gross_ms: gross,
        effective_ms: eff,
        dead_time_ms: dead,
        stoich_afr: stoich,
    })
}

#[tauri::command]
pub fn compute_fuel_pulse(req: FuelPulseRequest) -> Result<FuelPulseResult, String> {
    compute_fuel_pulse_inner(&req)
}

// ---------------------------------------------------------------------------
// Physics: charge temperature and air mass
// ---------------------------------------------------------------------------

/// Result of a speed-density air-mass estimate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AirMassResult {
    /// Estimated charge temperature, Kelvin.
    pub charge_temp_k: f64,
    /// Air mass per cylinder per intake stroke, grams.
    pub air_mass_g: f64,
    /// Volumetric efficiency implied by the inputs (ratio).
    pub implied_ve: f64,
}

/// Estimates air mass and charge temperature for one cylinder.
pub fn compute_air_mass_inner(
    displacement_cc: f64,
    cylinders: u32,
    map_kpa: f64,
    ve_ratio: f64,
    iat_c: f64,
    ect_c: f64,
    rpm: f64,
) -> Result<AirMassResult, String> {
    if !displacement_cc.is_finite() || displacement_cc <= 0.0 || cylinders == 0 {
        return Err("Displacement and cylinder count must be positive".to_string());
    }
    if !rpm.is_finite() || rpm <= 0.0 || !map_kpa.is_finite() || map_kpa <= 0.0 {
        return Err("RPM and MAP must be positive".to_string());
    }

    let engine = kivi_physics::thermo::SpeedDensityEngine::new(displacement_cc, cylinders);
    let air = engine.calculate_cylinder_air_mass_grams(map_kpa, ve_ratio, iat_c, ect_c, rpm);

    // Feed the computed air mass back as an equivalent MAF to report charge temp.
    let strokes_per_sec = (rpm / 60.0) * (cylinders as f64 / 2.0);
    let maf_gps = air * strokes_per_sec;
    let charge = engine
        .charge_temp_model
        .estimate_t_charge_k(iat_c, ect_c, maf_gps);

    let implied_ve = engine.calculate_ve_from_maf(maf_gps, map_kpa, iat_c, ect_c, rpm);

    if !air.is_finite() || !charge.is_finite() {
        return Err("Air-mass computation produced a non-finite value".to_string());
    }

    Ok(AirMassResult {
        charge_temp_k: charge,
        air_mass_g: air,
        implied_ve,
    })
}

#[tauri::command]
pub fn compute_air_mass(
    displacement_cc: f64,
    cylinders: u32,
    map_kpa: f64,
    ve_ratio: f64,
    iat_c: f64,
    ect_c: f64,
    rpm: f64,
) -> Result<AirMassResult, String> {
    compute_air_mass_inner(
        displacement_cc,
        cylinders,
        map_kpa,
        ve_ratio,
        iat_c,
        ect_c,
        rpm,
    )
}

// ---------------------------------------------------------------------------
// Physics: ignition timing
// ---------------------------------------------------------------------------

/// Result of a commanded spark-timing calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SparkTimingResult {
    /// Commanded spark advance, degrees BTDC.
    pub commanded_btdc: f64,
    /// Knock retard currently applied, degrees.
    pub knock_retard_deg: f64,
    /// MFB50 angle for the resulting timing, degrees ATDC.
    pub mfb50_atdc: f64,
}

/// Computes commanded spark timing with knock retard and hard limits applied.
pub fn compute_spark_timing_inner(
    base_btdc: f64,
    ect_correction: f64,
    iat_correction: f64,
    knock_retard_deg: f64,
) -> Result<SparkTimingResult, String> {
    if !base_btdc.is_finite() || !knock_retard_deg.is_finite() {
        return Err("Timing inputs must be finite".to_string());
    }
    if knock_retard_deg < 0.0 {
        return Err("Knock retard cannot be negative".to_string());
    }

    let mut controller = kivi_physics::ignition::IgnitionController::default();
    controller.knock_controller.current_retard_deg =
        knock_retard_deg.min(controller.knock_controller.max_retard_deg);

    let commanded = controller.calculate_commanded_spark_timing(
        base_btdc,
        ect_correction,
        iat_correction,
    );
    let mfb50 = controller.wiebe_model.calculate_mfb50_angle_atdc(commanded);

    Ok(SparkTimingResult {
        commanded_btdc: commanded,
        knock_retard_deg: controller.knock_controller.current_retard_deg,
        mfb50_atdc: mfb50,
    })
}

#[tauri::command]
pub fn compute_spark_timing(
    base_btdc: f64,
    ect_correction: f64,
    iat_correction: f64,
    knock_retard_deg: f64,
) -> Result<SparkTimingResult, String> {
    compute_spark_timing_inner(base_btdc, ect_correction, iat_correction, knock_retard_deg)
}

// ---------------------------------------------------------------------------
// Flashing: read-only plan description
// ---------------------------------------------------------------------------

/// One physical flash block, as reported to the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlashBlockView {
    /// Start offset within the chip.
    pub start_address: u32,
    /// Length in bytes.
    pub length: u32,
    /// Block role (`Boot`, `Parameter`, `Calibration`, `OperatingSystem`).
    pub block_type: String,
    /// Whether writing this block is safe.
    pub write_safe: bool,
}

/// A chip description for the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlashTargetView {
    /// Chip model name.
    pub model_name: String,
    /// Total declared size in bytes.
    pub total_size_bytes: u32,
    /// Sum of block lengths (may differ from `total_size_bytes`; see the
    /// `Am29F400BT` defect preserved from the source).
    pub blocks_total_bytes: u64,
    /// Blocks in address order.
    pub blocks: Vec<FlashBlockView>,
}

/// Describes a flash chip by JEDEC id.
///
/// **Read-only.** This reports what the chip layout is; it does not flash
/// anything and confers no write capability.
pub fn describe_flash_target_view_inner(chip_id: u32) -> Result<FlashTargetView, String> {
    let info = kivi_core::flash_target::describe_flash_target(chip_id)
        .map_err(|e| e.to_string())?;

    let blocks: Vec<FlashBlockView> = info
        .blocks
        .iter()
        .map(|b| FlashBlockView {
            start_address: b.start_address,
            length: b.length,
            block_type: format!("{:?}", b.block_type),
            write_safe: kivi_core::flash_target::is_block_write_safe(b),
        })
        .collect();

    let blocks_total_bytes: u64 = info.blocks.iter().map(|b| b.length as u64).sum();

    Ok(FlashTargetView {
        model_name: info.model_name.to_string(),
        total_size_bytes: info.total_size_bytes,
        blocks_total_bytes,
        blocks,
    })
}

#[tauri::command]
pub fn describe_flash_target_view(chip_id: u32) -> Result<FlashTargetView, String> {
    describe_flash_target_view_inner(chip_id)
}

// ---------------------------------------------------------------------------
// Telemetry: histogram hit-tracing
// ---------------------------------------------------------------------------

/// A histogram cell hit count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistogramCell {
    /// Row index.
    pub row: usize,
    /// Column index.
    pub col: usize,
    /// Number of samples that landed in this cell.
    pub hits: u32,
}

/// Result of histogram hit-tracing over a list of operating points.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistogramResult {
    /// Row axis used.
    pub row_axis: Vec<f64>,
    /// Column axis used.
    pub col_axis: Vec<f64>,
    /// Cells that received at least one hit, in row-major order.
    pub cells: Vec<HistogramCell>,
    /// Total samples ingested.
    pub total_samples: usize,
}

/// Builds a hit histogram from operating points.
///
/// Points outside the axes are counted as misses, matching the source's
/// `locate_cell` behaviour.
pub fn compute_histogram_inner(
    row_axis: Vec<f64>,
    col_axis: Vec<f64>,
    points: &[(f64, f64)],
) -> Result<HistogramResult, String> {
    if row_axis.len() < 2 || col_axis.len() < 2 {
        return Err("Both axes need at least two breakpoints".to_string());
    }
    if points.is_empty() {
        return Err("No operating points supplied".to_string());
    }

    let mut hist = kivi_telemetry::histogram::HistogramTracer::new(
        row_axis.clone(),
        col_axis.clone(),
    );

    // `ingest_sample(row, col, feedback)` also accumulates a running average, so
    // the feedback value is the sample itself here.
    for &(x, y) in points {
        if x.is_finite() && y.is_finite() {
            hist.ingest_sample(x, y, 0.0);
        }
    }

    let mut cells = Vec::new();
    for (r, row) in hist.cells.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if cell.hit_count > 0 {
                cells.push(HistogramCell {
                    row: r,
                    col: c,
                    hits: cell.hit_count,
                });
            }
        }
    }

    Ok(HistogramResult {
        row_axis,
        col_axis,
        cells,
        total_samples: points.len(),
    })
}

#[tauri::command]
pub fn compute_histogram(
    row_axis: Vec<f64>,
    col_axis: Vec<f64>,
    points: Vec<(f64, f64)>,
) -> Result<HistogramResult, String> {
    compute_histogram_inner(row_axis, col_axis, &points)
}

// ---------------------------------------------------------------------------
// Physics: torque structure
// ---------------------------------------------------------------------------

/// Result of a torque-structure calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TorqueResult {
    /// Driver-requested torque, Nm.
    pub driver_torque_nm: f64,
    /// Friction loss, Nm.
    pub friction_nm: f64,
    /// Pumping loss, Nm.
    pub pumping_nm: f64,
    /// Required indicated torque, Nm.
    pub indicated_nm: f64,
    /// Spark retard needed for a fast-path torque cut, degrees.
    pub fast_path_retard_deg: f64,
}

/// Computes the torque structure for a pedal/rpm/MAP operating point.
pub fn compute_torque_inner(
    pedal_percent: f64,
    rpm: f64,
    map_kpa: f64,
    ect_c: f64,
    accessory_load_nm: f64,
    target_torque_nm: f64,
) -> Result<TorqueResult, String> {
    if !rpm.is_finite() || rpm < 0.0 {
        return Err("RPM cannot be negative".to_string());
    }
    let engine = kivi_physics::torque::TorqueStructureEngine::default();

    let driver = engine.calculate_driver_requested_torque(pedal_percent, rpm);
    let friction = engine.estimate_friction_loss_torque(ect_c, rpm);
    let pumping = engine.estimate_pumping_loss_torque(map_kpa);
    let indicated =
        engine.calculate_target_indicated_torque(driver, friction, pumping, accessory_load_nm);
    let retard = engine.calculate_fast_path_spark_retard(target_torque_nm, driver);

    Ok(TorqueResult {
        driver_torque_nm: driver,
        friction_nm: friction,
        pumping_nm: pumping,
        indicated_nm: indicated,
        fast_path_retard_deg: retard,
    })
}

#[tauri::command]
pub fn compute_torque(
    pedal_percent: f64,
    rpm: f64,
    map_kpa: f64,
    ect_c: f64,
    accessory_load_nm: f64,
    target_torque_nm: f64,
) -> Result<TorqueResult, String> {
    compute_torque_inner(
        pedal_percent,
        rpm,
        map_kpa,
        ect_c,
        accessory_load_nm,
        target_torque_nm,
    )
}

// ---------------------------------------------------------------------------
// Sensors: decode a raw ADC / voltage into engineering units
// ---------------------------------------------------------------------------

/// Result of a sensor decode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SensorDecodeResult {
    /// Decoded engineering value.
    pub value: f64,
    /// Unit of the decoded value.
    pub unit: String,
}

/// Decodes a raw sensor reading.
///
/// `kind` is `map` (volts → kPa), `tps` (ADC → %), or `thermistor`
/// (ADC → °C).
pub fn decode_sensor_inner(kind: &str, raw: f64) -> Result<SensorDecodeResult, String> {
    match kind.to_ascii_lowercase().as_str() {
        "map" => {
            let s = kivi_physics::sensors::MapSensor::default();
            Ok(SensorDecodeResult {
                value: s.voltage_to_kpa(raw),
                unit: "kPa".to_string(),
            })
        }
        "tps" => {
            let s = kivi_physics::sensors::ThrottlePositionSensor::default();
            let adc = raw.max(0.0).min(u32::MAX as f64) as u32;
            Ok(SensorDecodeResult {
                value: s.adc_to_percent(adc),
                unit: "%".to_string(),
            })
        }
        "thermistor" => {
            let c = kivi_physics::sensors::AdcThermistorCircuit::default();
            let adc = raw.max(0.0).min(u32::MAX as f64) as u32;
            Ok(SensorDecodeResult {
                value: c.adc_to_celsius(adc),
                unit: "°C".to_string(),
            })
        }
        other => Err(format!(
            "Unknown sensor kind '{}' (expected map, tps, thermistor)",
            other
        )),
    }
}

#[tauri::command]
pub fn decode_sensor(kind: String, raw: f64) -> Result<SensorDecodeResult, String> {
    decode_sensor_inner(&kind, raw)
}

// ---------------------------------------------------------------------------
// Chemistry: flex-fuel blend, compressor, EGT
// ---------------------------------------------------------------------------

/// Result of a flex-fuel blend calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlendResult {
    /// Stoichiometric AFR of the blend.
    pub stoich_afr: f64,
    /// Fuel volume multiplier versus gasoline.
    pub volume_multiplier: f64,
    /// Latent heat of vaporisation, kJ/kg.
    pub latent_heat_kj_kg: f64,
}

/// Computes flex-fuel blend properties for an ethanol percentage.
pub fn compute_blend_inner(ethanol_percent: f64) -> Result<BlendResult, String> {
    if !ethanol_percent.is_finite() || !(0.0..=100.0).contains(&ethanol_percent) {
        return Err("Ethanol percentage must be between 0 and 100".to_string());
    }
    let afr = kivi_physics::chemistry::FlexFuelCalculator::calculate_blend_stoich_afr(
        ethanol_percent,
    );
    let mult = kivi_physics::chemistry::FlexFuelCalculator::calculate_fuel_volume_multiplier(
        ethanol_percent,
    );
    // Interpolate latent heat between pure gasoline (350) and pure ethanol (840).
    let e = ethanol_percent / 100.0;
    let latent = 350.0 * (1.0 - e) + 840.0 * e;

    Ok(BlendResult {
        stoich_afr: afr,
        volume_multiplier: mult,
        latent_heat_kj_kg: latent,
    })
}

#[tauri::command]
pub fn compute_blend(ethanol_percent: f64) -> Result<BlendResult, String> {
    compute_blend_inner(ethanol_percent)
}

/// Result of a turbo compressor calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompressorResult {
    /// Discharge temperature, °C.
    pub discharge_c: f64,
    /// Intercooler outlet temperature, °C.
    pub intercooler_out_c: f64,
    /// Estimated exhaust gas temperature, °C.
    pub egt_c: f64,
}

/// Computes compressor discharge, intercooler outlet, and EGT.
pub fn compute_compressor_inner(
    t_in_c: f64,
    pressure_ratio: f64,
    isentropic_efficiency: f64,
    intercooler_efficiency: f64,
    ambient_c: f64,
    lambda: f64,
    spark_retard_deg: f64,
) -> Result<CompressorResult, String> {
    let discharge = kivi_physics::chemistry::TurbochargerThermodynamics::
        compressor_discharge_temp_celsius(t_in_c, pressure_ratio, isentropic_efficiency)
        .map_err(|e| e.to_string())?;

    let intercooler_out = kivi_physics::chemistry::TurbochargerThermodynamics::
        intercooler_outlet_temp_celsius(discharge, ambient_c, intercooler_efficiency);

    let egt = kivi_physics::chemistry::ExhaustThermalProtection::estimate_egt_celsius(
        850.0,
        lambda,
        spark_retard_deg,
    );

    Ok(CompressorResult {
        discharge_c: discharge,
        intercooler_out_c: intercooler_out,
        egt_c: egt,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn compute_compressor(
    t_in_c: f64,
    pressure_ratio: f64,
    isentropic_efficiency: f64,
    intercooler_efficiency: f64,
    ambient_c: f64,
    lambda: f64,
    spark_retard_deg: f64,
) -> Result<CompressorResult, String> {
    compute_compressor_inner(
        t_in_c,
        pressure_ratio,
        isentropic_efficiency,
        intercooler_efficiency,
        ambient_c,
        lambda,
        spark_retard_deg,
    )
}

// ---------------------------------------------------------------------------
// Transients: DFCO, anti-stall, MAF linearisation
// ---------------------------------------------------------------------------

/// Result of a transient-state evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransientResult {
    /// DFCO fuel-cut multiplier (0.0 = cut, 1.0 = full fuel).
    pub dfco_multiplier: f64,
    /// Anti-stall spark adder, degrees.
    pub anti_stall_adder_deg: f64,
    /// MAF airflow for the supplied sensor voltage, g/s.
    pub maf_airflow_gps: f64,
}

/// Evaluates DFCO, anti-stall and MAF for one operating point.
pub fn compute_transient_inner(
    rpm: f64,
    tps_pct: f64,
    speed_kph: f64,
    ect_c: f64,
    target_idle_rpm: f64,
    maf_volts: f64,
) -> Result<TransientResult, String> {
    if !rpm.is_finite() || rpm < 0.0 {
        return Err("RPM must be a finite non-negative number".to_string());
    }

    let mut dfco = kivi_physics::transients::DfcoController::default();
    let multiplier = dfco.update(rpm, tps_pct, speed_kph, ect_c);

    let anti_stall = kivi_physics::transients::AntiStallSaver::default();
    let adder = anti_stall.calculate_spark_adder(rpm, target_idle_rpm);

    let maf = kivi_physics::transients::PiecewiseMafLinearizer::default();
    let airflow = maf.voltage_to_airflow_gps(maf_volts);

    Ok(TransientResult {
        dfco_multiplier: multiplier,
        anti_stall_adder_deg: adder,
        maf_airflow_gps: airflow,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn compute_transient(
    rpm: f64,
    tps_pct: f64,
    speed_kph: f64,
    ect_c: f64,
    target_idle_rpm: f64,
    maf_volts: f64,
) -> Result<TransientResult, String> {
    compute_transient_inner(rpm, tps_pct, speed_kph, ect_c, target_idle_rpm, maf_volts)
}

// ---------------------------------------------------------------------------
// Axis rescaling: injector swap and 1D resample
// ---------------------------------------------------------------------------

/// Result of an injector rescale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InjectorRescaleResult {
    /// New pulse width, ms.
    pub new_pulse_width_ms: f64,
}

/// Rescales a pulse width for a different injector flow rate.
pub fn rescale_injector_inner(
    pw_old_ms: f64,
    dead_old_ms: f64,
    flow_old_cc: f64,
    flow_new_cc: f64,
    dead_new_ms: f64,
) -> Result<InjectorRescaleResult, String> {
    let v = kivi_physics::axis_rescaling::InjectorScaler::rescale_pulse_width(
        pw_old_ms,
        dead_old_ms,
        flow_old_cc,
        flow_new_cc,
        dead_new_ms,
    )
    .map_err(|e| e.to_string())?;
    Ok(InjectorRescaleResult { new_pulse_width_ms: v })
}

#[tauri::command]
pub fn rescale_injector(
    pw_old_ms: f64,
    dead_old_ms: f64,
    flow_old_cc: f64,
    flow_new_cc: f64,
    dead_new_ms: f64,
) -> Result<InjectorRescaleResult, String> {
    rescale_injector_inner(pw_old_ms, dead_old_ms, flow_old_cc, flow_new_cc, dead_new_ms)
}

/// Resamples a 1D curve onto a new axis.
pub fn resample_axis_inner(
    old_axis: Vec<f64>,
    old_values: Vec<f64>,
    new_axis: Vec<f64>,
) -> Result<Vec<f64>, String> {
    kivi_physics::axis_rescaling::AxisResampler::resample_1d(&old_axis, &old_values, &new_axis)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resample_axis(
    old_axis: Vec<f64>,
    old_values: Vec<f64>,
    new_axis: Vec<f64>,
) -> Result<Vec<f64>, String> {
    resample_axis_inner(old_axis, old_values, new_axis)
}

// ---------------------------------------------------------------------------
// DTC: parse, debounce, switch patch
// ---------------------------------------------------------------------------

/// A parsed diagnostic trouble code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DtcView {
    /// Normalised 5-character code (e.g. `P0420`).
    pub code: String,
    /// Category letter (`P`, `C`, `B`, `U`).
    pub category: String,
    /// Whether the code is SAE-standard rather than manufacturer-specific.
    pub is_standard: bool,
}

/// Parses and normalises a DTC string.
pub fn parse_dtc_inner(text: &str) -> Result<DtcView, String> {
    let c = kivi_physics::dtc::DtcCode::parse(text).map_err(|e| e.to_string())?;
    Ok(DtcView {
        code: c.to_string(),
        category: match c.category {
            kivi_physics::dtc::DtcCategory::Powertrain => "P",
            kivi_physics::dtc::DtcCategory::Chassis => "C",
            kivi_physics::dtc::DtcCategory::Body => "B",
            kivi_physics::dtc::DtcCategory::Network => "U",
        }
        .to_string(),
        is_standard: c.is_standard,
    })
}

#[tauri::command]
pub fn parse_dtc(text: String) -> Result<DtcView, String> {
    parse_dtc_inner(&text)
}

/// Applies a DTC-disable patch to a ROM copy and reports the byte changed.
///
/// **Returns a new buffer.** The caller decides whether to keep it; nothing is
/// written to disk or to a device.
pub fn patch_dtc_disable_inner(
    rom: Vec<u8>,
    switch_address: usize,
) -> Result<Vec<u8>, String> {
    let mut out = rom;
    kivi_physics::dtc::DtcSwitchMatrix::patch_dtc_disable(&mut out, switch_address)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[tauri::command]
pub fn patch_dtc_disable(rom: Vec<u8>, switch_address: usize) -> Result<Vec<u8>, String> {
    patch_dtc_disable_inner(rom, switch_address)
}

// ---------------------------------------------------------------------------
// Airflow: compressible throttle mass flow
// ---------------------------------------------------------------------------

/// Result of a compressible throttle-flow calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThrottleFlowResult {
    /// Mass flow through the throttle, kg/s.
    pub mass_flow_kg_s: f64,
    /// Pressure ratio (manifold / ambient).
    pub pressure_ratio: f64,
    /// Whether the flow is choked (sonic at the throat).
    pub is_choked: bool,
    /// The critical pressure ratio for air.
    pub critical_ratio: f64,
}

/// Computes compressible mass flow through a throttle body.
///
/// Uses the isentropic orifice relation with the source's choked-flow factor.
pub fn compute_throttle_flow_inner(
    effective_area_m2: f64,
    ambient_pa: f64,
    ambient_k: f64,
    manifold_pa: f64,
) -> Result<ThrottleFlowResult, String> {
    if !effective_area_m2.is_finite() || effective_area_m2 <= 0.0 {
        return Err("Effective area must be a finite positive number".to_string());
    }
    if !ambient_pa.is_finite() || ambient_pa <= 0.0 {
        return Err("Ambient pressure must be a finite positive number".to_string());
    }
    if !ambient_k.is_finite() || ambient_k < 200.0 {
        return Err("Ambient temperature must be at least 200 K".to_string());
    }
    if !manifold_pa.is_finite() || manifold_pa < 0.0 {
        return Err("Manifold pressure must be a finite non-negative number".to_string());
    }

    let flow = kivi_physics::airflow_dynamics::CompressibleThrottleFlow::
        calculate_throttle_mass_flow(effective_area_m2, ambient_pa, ambient_k, manifold_pa);
    let ratio = manifold_pa / ambient_pa;
    let critical = kivi_physics::airflow_dynamics::CompressibleThrottleFlow::
        critical_pressure_ratio();

    if !flow.is_finite() {
        return Err("Throttle flow computation produced a non-finite value".to_string());
    }

    Ok(ThrottleFlowResult {
        mass_flow_kg_s: flow,
        pressure_ratio: ratio,
        is_choked: ratio <= critical,
        critical_ratio: critical,
    })
}

#[tauri::command]
pub fn compute_throttle_flow(
    effective_area_m2: f64,
    ambient_pa: f64,
    ambient_k: f64,
    manifold_pa: f64,
) -> Result<ThrottleFlowResult, String> {
    compute_throttle_flow_inner(effective_area_m2, ambient_pa, ambient_k, manifold_pa)
}

// ---------------------------------------------------------------------------
// Calculation presets
// ---------------------------------------------------------------------------

/// A saved calculation preset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalcPreset {
    /// Preset name.
    pub name: String,
    /// Tab the preset belongs to.
    pub tab: String,
    /// Field values keyed by input name.
    pub values: std::collections::BTreeMap<String, f64>,
}

/// Validates a preset before it is stored or applied.
///
/// A preset with an empty name or an unknown tab is rejected rather than stored
/// as a silently unusable entry.
pub fn validate_preset_inner(preset: &CalcPreset, known_tabs: &[&str]) -> Result<(), String> {
    if preset.name.trim().is_empty() {
        return Err("Preset name cannot be empty".to_string());
    }
    if preset.name.len() > 64 {
        return Err("Preset name is too long (max 64 characters)".to_string());
    }
    if !known_tabs.contains(&preset.tab.as_str()) {
        return Err(format!(
            "Unknown tab '{}' (expected one of: {})",
            preset.tab,
            known_tabs.join(", ")
        ));
    }
    if preset.values.is_empty() {
        return Err("A preset must carry at least one value".to_string());
    }
    for (k, v) in &preset.values {
        if !v.is_finite() {
            return Err(format!("Preset value '{}' is not a finite number", k));
        }
    }
    Ok(())
}
