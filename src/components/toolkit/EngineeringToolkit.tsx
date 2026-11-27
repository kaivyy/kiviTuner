import React, { useState, useCallback, useRef } from 'react';
import { Calculator, Gauge, Cpu, AlertTriangle } from 'lucide-react';
import { FloatingWindow, useFloatingWindowContext } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import type {
  AirMassResult,
  HistogramResult,
  DtcView,
  FlashTargetView,
  FuelPulseResult,
  SensorDecodeResult,
  SparkTimingResult,
  TorqueResult,
} from '../../lib/tauriBridge';
import type { BlendResult, CompressorResult } from './chemistryController';
import type { RescaleResult, TransientResult } from './transientController';
import { runBlend, runCompressor, runThrottleFlow } from './chemistryController';
import type { ThrottleFlowResult } from './chemistryController';
import { runInjectorRescale, runTransient } from './transientController';
import { runDtcParse } from './dtcController';
import {
  runAirMass,
  runDescribeFlashTarget,
  runHistogram,
  runFuelPulse,
  runSensorDecode,
  runSparkTiming,
  runTorque,
  describeFlashTarget,
  hasBlockSizeMismatch,
  type ToolkitState,
} from './toolkitController';

export interface EngineeringToolkitProps {
  isOpen: boolean;
  onClose: () => void;
  standalone?: boolean;
}

type Tab =
  | 'fuel'
  | 'air'
  | 'timing'
  | 'torque'
  | 'sensor'
  | 'chip'
  | 'histogram'
  | 'chemistry'
  | 'transient'
  | 'rescale'
  | 'dtc'
  | 'airflow';

const TABS: Array<{ id: Tab; label: string; category: string }> = [
  { id: 'fuel', label: 'Fuel pulse', category: 'Fueling & Airflow' },
  { id: 'air', label: 'Air mass', category: 'Fueling & Airflow' },
  { id: 'airflow', label: 'Throttle flow', category: 'Fueling & Airflow' },
  { id: 'rescale', label: 'Rescale', category: 'Fueling & Airflow' },
  { id: 'timing', label: 'Spark timing', category: 'Ignition & Torque' },
  { id: 'torque', label: 'Torque', category: 'Ignition & Torque' },
  { id: 'transient', label: 'Transients', category: 'Ignition & Torque' },
  { id: 'chemistry', label: 'Chemistry', category: 'Boost & Fuel' },
  { id: 'sensor', label: 'Sensor', category: 'Diagnostics & Flash' },
  { id: 'chip', label: 'Flash chip', category: 'Diagnostics & Flash' },
  { id: 'histogram', label: 'Hit map', category: 'Diagnostics & Flash' },
  { id: 'dtc', label: 'DTC', category: 'Diagnostics & Flash' },
];

/** Single-line numeric field with a non-wrapping label and monospace input. */
const Field: React.FC<{
  label: string;
  value: string;
  onChange: (v: string) => void;
  ariaLabel: string;
}> = ({ label, value, onChange, ariaLabel }) => (
  <label className="flex items-center justify-between gap-3 rounded border border-kiviBorder/70 bg-kiviBg/40 px-2.5 py-1.5 text-[11px] text-kiviMuted">
    <span className="whitespace-nowrap">{label}</span>
    <input
      type="text"
      inputMode="decimal"
      aria-label={ariaLabel}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-28 shrink-0 rounded border border-kiviBorder bg-kiviSurface px-2 py-0.5 text-right font-mono text-kiviInk"
    />
  </label>
);

/** Renders a toolkit result or its error in a crisp output card. */
function ResultBlock<T>({
  state,
  render,
}: {
  state: ToolkitState<T> | null;
  render: (d: T) => React.ReactNode;
}) {
  return (
    <div className="rounded-lg border border-kiviBorder bg-kiviBg/60 p-3 text-[11px]">
      {!state ? (
        <p className="text-kiviMuted">Enter values and run the calculation.</p>
      ) : state.kind === 'no-backend' ? (
        <p className="text-kiviWarn">{state.message}</p>
      ) : state.kind === 'error' ? (
        <p className="text-kiviRed">{state.message}</p>
      ) : (
        <div className="text-kiviInk">{render(state.data)}</div>
      )}
    </div>
  );
}

/**
 * Engineering toolkit panel.
 *
 * Calculation-only tools a tuner uses while calibrating: injector pulse width,
 * cylinder air mass, commanded spark timing, torque structure, sensor decode,
 * and flash-chip layout inspection.
 *
 * Nothing here writes to a ROM, an ECU, or a file.
 */
const ToolkitResponsiveLayout: React.FC<{
  tab: Tab;
  setTab: (t: Tab) => void;
  activeTabLabel: string;
  activeTabCategory: string;
  children: React.ReactNode;
}> = ({ tab, setTab, activeTabLabel, activeTabCategory, children }) => {
  const { isCompact } = useFloatingWindowContext();
  return (
    <>
      {isCompact && (
        <div
          role="tablist"
          aria-label="Engineering calculator tools compact"
          className="no-scrollbar flex shrink-0 items-center gap-1.5 overflow-x-auto border-b border-kiviBorder bg-kiviBg px-3 py-2"
        >
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => setTab(t.id)}
              className={`shrink-0 whitespace-nowrap rounded-md border px-2.5 py-1 text-[11px] font-medium transition-colors ${
                tab === t.id
                  ? 'border-kiviAccent/50 bg-kiviAccent/15 text-kiviAccent'
                  : 'border-kiviBorder bg-kiviSurface text-kiviSecondary hover:text-kiviInk'
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>
      )}

      <div className="flex min-h-0 flex-1 flex-row overflow-hidden">
        {!isCompact && (
          <aside
            role="tablist"
            aria-label="Engineering calculator tools"
            aria-orientation="vertical"
            className="flex w-44 shrink-0 flex-col gap-1 overflow-y-auto border-r border-kiviBorder bg-kiviBg p-2"
          >
            {TABS.map((t) => (
              <button
                key={t.id}
                type="button"
                role="tab"
                aria-selected={tab === t.id}
                onClick={() => setTab(t.id)}
                className={`w-full shrink-0 whitespace-nowrap rounded-md border px-2.5 py-1.5 text-left text-[11px] font-medium transition-colors ${
                  tab === t.id
                    ? 'border-kiviAccent/50 bg-kiviAccent/15 text-kiviAccent'
                    : 'border-transparent text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                }`}
              >
                {t.label}
              </button>
            ))}
          </aside>
        )}

        <div className="min-w-0 flex-1 space-y-4 overflow-y-auto p-4 sm:p-5">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-kiviBorder pb-2">
            <h3 className="whitespace-nowrap text-xs font-semibold uppercase tracking-wide text-kiviInk">
              {activeTabLabel}
            </h3>
            <span className="whitespace-nowrap rounded border border-kiviBorder bg-kiviElevated px-2 py-0.5 text-[10px] text-kiviMuted">
              {activeTabCategory}
            </span>
          </div>
          {children}
        </div>
      </div>
    </>
  );
};

export const EngineeringToolkit: React.FC<EngineeringToolkitProps> = ({
  isOpen,
  onClose,
  standalone = false,
}) => {
  const [tab, setTab] = useState<Tab>('fuel');
  const reqSeqRef = useRef(0);

  // fuel
  const [airMass, setAirMass] = useState('0.15');
  const [lambda, setLambda] = useState('0.88');
  const [volts, setVolts] = useState('13.8');
  const [railKpa, setRailKpa] = useState('300');
  const [cycleDt, setCycleDt] = useState('0.017');
  const [fuel, setFuel] = useState('e10');
  const [fuelState, setFuelState] = useState<ToolkitState<FuelPulseResult> | null>(null);

  // air
  const [disp, setDisp] = useState('155');
  const [cyls, setCyls] = useState('1');
  const [mapKpa, setMapKpa] = useState('100');
  const [ve, setVe] = useState('0.85');
  const [iat, setIat] = useState('30');
  const [ect, setEct] = useState('85');
  const [rpm, setRpm] = useState('7000');
  const [airState, setAirState] = useState<ToolkitState<AirMassResult> | null>(null);

  // timing
  const [baseBtdc, setBaseBtdc] = useState('20');
  const [ectCorr, setEctCorr] = useState('3');
  const [iatCorr, setIatCorr] = useState('-2');
  const [knock, setKnock] = useState('0');
  const [timingState, setTimingState] = useState<ToolkitState<SparkTimingResult> | null>(null);

  // torque
  const [pedal, setPedal] = useState('100');
  const [tqRpm, setTqRpm] = useState('5000');
  const [tqMap, setTqMap] = useState('100');
  const [tqEct, setTqEct] = useState('85');
  const [accessory, setAccessory] = useState('5');
  const [targetTq, setTargetTq] = useState('100');
  const [torqueState, setTorqueState] = useState<ToolkitState<TorqueResult> | null>(null);

  // sensor
  const [sensorKind, setSensorKind] = useState('map');
  const [sensorRaw, setSensorRaw] = useState('4.5');
  const [sensorState, setSensorState] = useState<ToolkitState<SensorDecodeResult> | null>(null);

  // chip
  const [chipId, setChipId] = useState('0x00012258');
  const [chipState, setChipState] = useState<ToolkitState<FlashTargetView> | null>(null);

  // histogram
  const [histRows, setHistRows] = useState('0,1000,2000,3000,4000,5000,6000,7000');
  const [histCols, setHistCols] = useState('0,20,40,60,80,100');
  const [histPoints, setHistPoints] = useState('1500,30\n1500,30\n2500,50\n4500,70');
  const [histState, setHistState] = useState<ToolkitState<HistogramResult> | null>(null);

  // chemistry
  const [ethanol, setEthanol] = useState('50');
  const [chemState, setChemState] = useState<ToolkitState<BlendResult> | null>(null);
  const [compTIn, setCompTIn] = useState('30');
  const [compPr, setCompPr] = useState('2.0');
  const [compEta, setCompEta] = useState('0.72');
  const [compIc, setCompIc] = useState('0.80');
  const [compAmb, setCompAmb] = useState('30');
  const [compLambda, setCompLambda] = useState('0.85');
  const [compRetard, setCompRetard] = useState('0');
  const [compState, setCompState] = useState<ToolkitState<CompressorResult> | null>(null);

  // transients
  const [trRpm, setTrRpm] = useState('2000');
  const [trTps, setTrTps] = useState('0');
  const [trSpeed, setTrSpeed] = useState('40');
  const [trEct, setTrEct] = useState('85');
  const [trIdle, setTrIdle] = useState('900');
  const [trMaf, setTrMaf] = useState('1.5');
  const [trState, setTrState] = useState<ToolkitState<TransientResult> | null>(null);

  // rescale
  const [rsPw, setRsPw] = useState('5.0');
  const [rsDeadOld, setRsDeadOld] = useState('1.0');
  const [rsFlowOld, setRsFlowOld] = useState('200');
  const [rsFlowNew, setRsFlowNew] = useState('400');
  const [rsDeadNew, setRsDeadNew] = useState('1.0');
  const [rsState, setRsState] = useState<ToolkitState<RescaleResult> | null>(null);

  // dtc
  const [dtcText, setDtcText] = useState('P0420');
  const [dtcState, setDtcState] = useState<ToolkitState<DtcView> | null>(null);

  // airflow
  const [tfAngle, setTfAngle] = useState('30');
  const [tfRpm, setTfRpm] = useState('3000');
  const [tfMap, setTfMap] = useState('50');
  const [tfIat, setTfIat] = useState('30');
  const [tfState, setTfState] = useState<ToolkitState<ThrottleFlowResult> | null>(null);

  const num = (s: string): number => Number(s.trim());

  const handleFuel = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runFuelPulse({
      air_mass_g: num(airMass),
      target_lambda: num(lambda),
      battery_volts: num(volts),
      rail_diff_kpa: num(railKpa),
      cycle_dt_sec: num(cycleDt),
      fuel,
    });
    if (seq === reqSeqRef.current) setFuelState(res);
  }, [airMass, lambda, volts, railKpa, cycleDt, fuel]);

  const handleAir = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runAirMass(num(disp), num(cyls), num(mapKpa), num(ve), num(iat), num(ect), num(rpm));
    if (seq === reqSeqRef.current) setAirState(res);
  }, [disp, cyls, mapKpa, ve, iat, ect, rpm]);

  const handleTiming = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runSparkTiming(num(baseBtdc), num(ectCorr), num(iatCorr), num(knock));
    if (seq === reqSeqRef.current) setTimingState(res);
  }, [baseBtdc, ectCorr, iatCorr, knock]);

  const handleTorque = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runTorque(num(pedal), num(tqRpm), num(tqMap), num(tqEct), num(accessory), num(targetTq));
    if (seq === reqSeqRef.current) setTorqueState(res);
  }, [pedal, tqRpm, tqMap, tqEct, accessory, targetTq]);

  const handleSensor = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runSensorDecode(sensorKind, num(sensorRaw));
    if (seq === reqSeqRef.current) setSensorState(res);
  }, [sensorKind, sensorRaw]);

  const handleChip = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const id = chipId.trim().toLowerCase().startsWith('0x')
      ? parseInt(chipId.trim(), 16)
      : Number(chipId);
    const res = await runDescribeFlashTarget(id);
    if (seq === reqSeqRef.current) setChipState(res);
  }, [chipId]);

  const handleHistogram = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const parseAxis = (text: string): number[] =>
      text
        .split(',')
        .map((t) => Number(t.trim()))
        .filter((n) => Number.isFinite(n));

    const points: Array<[number, number]> = histPoints
      .split('\n')
      .map((line) => line.split(',').map((t) => Number(t.trim())))
      .filter((p) => p.length === 2 && p.every((n) => Number.isFinite(n)))
      .map((p) => [p[0], p[1]] as [number, number]);

    const res = await runHistogram(parseAxis(histRows), parseAxis(histCols), points);
    if (seq === reqSeqRef.current) setHistState(res);
  }, [histRows, histCols, histPoints]);

  const handleBlend = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runBlend(num(ethanol));
    if (seq === reqSeqRef.current) setChemState(res);
  }, [ethanol]);

  const handleCompressor = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runCompressor({
      tInC: num(compTIn),
      pressureRatio: num(compPr),
      isentropicEfficiency: num(compEta),
      intercoolerEfficiency: num(compIc),
      ambientC: num(compAmb),
      lambda: num(compLambda),
      sparkRetardDeg: num(compRetard),
    });
    if (seq === reqSeqRef.current) setCompState(res);
  }, [compTIn, compPr, compEta, compIc, compAmb, compLambda, compRetard]);

  const handleTransient = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runTransient({
      rpm: num(trRpm),
      tpsPct: num(trTps),
      speedKph: num(trSpeed),
      ectC: num(trEct),
      targetIdleRpm: num(trIdle),
      mafVolts: num(trMaf),
    });
    if (seq === reqSeqRef.current) setTrState(res);
  }, [trRpm, trTps, trSpeed, trEct, trIdle, trMaf]);

  const handleRescale = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runInjectorRescale({
      pwOldMs: num(rsPw),
      deadOldMs: num(rsDeadOld),
      flowOldCc: num(rsFlowOld),
      flowNewCc: num(rsFlowNew),
      deadNewMs: num(rsDeadNew),
    });
    if (seq === reqSeqRef.current) setRsState(res);
  }, [rsPw, rsDeadOld, rsFlowOld, rsFlowNew, rsDeadNew]);

  const handleDtc = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runDtcParse(dtcText);
    if (seq === reqSeqRef.current) setDtcState(res);
  }, [dtcText]);

  const handleThrottleFlow = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    const res = await runThrottleFlow({
      throttleDeg: num(tfAngle),
      rpm: num(tfRpm),
      mapKpa: num(tfMap),
      iatC: num(tfIat),
    });
    if (seq === reqSeqRef.current) setTfState(res);
  }, [tfAngle, tfRpm, tfMap, tfIat]);

  const activeTabMeta = TABS.find((t) => t.id === tab) ?? TABS[0];
  const spec = TOOL_WINDOW_SPECS.toolkit;

  return (
    <FloatingWindow
      windowId="toolkit"
      isOpen={isOpen}
      onClose={onClose}
      title="Engineering Toolkit"
      titleId="engineering-toolkit-title"
      badge="calculation only"
      icon={<Calculator className="h-4 w-4" />}
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      standalone={standalone}
      popoutToolId="toolkit"
      titlebarTone="sidebar"
      closeLabel="Close engineering toolkit"
      footer={
        <div className="flex shrink-0 items-center gap-2 border-t border-kiviBorder bg-kiviBg px-4 py-2 text-[10px] text-kiviMuted">
          <Gauge className="h-3.5 w-3.5 shrink-0" />
          <span>
            All calculations are read-only. Nothing here writes to a ROM, an ECU, or a file.
          </span>
        </div>
      }
    >
      <ToolkitResponsiveLayout
        tab={tab}
        setTab={setTab}
        activeTabLabel={activeTabMeta.label}
        activeTabCategory={activeTabMeta.category}
      >

            {tab === 'fuel' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="Air mass (g)" value={airMass} onChange={setAirMass} ariaLabel="Fuel air mass" />
                  <Field label="Target lambda" value={lambda} onChange={setLambda} ariaLabel="Fuel lambda" />
                  <Field label="Battery (V)" value={volts} onChange={setVolts} ariaLabel="Fuel battery volts" />
                  <Field label="Rail ΔP (kPa)" value={railKpa} onChange={setRailKpa} ariaLabel="Fuel rail pressure" />
                  <Field label="Cycle dt (s)" value={cycleDt} onChange={setCycleDt} ariaLabel="Fuel cycle time" />
                  <label className="flex items-center justify-between gap-3 rounded border border-kiviBorder/70 bg-kiviBg/40 px-2.5 py-1.5 text-[11px] text-kiviMuted">
                    <span className="whitespace-nowrap">Fuel type</span>
                    <select
                      aria-label="Fuel type"
                      value={fuel}
                      onChange={(e) => setFuel(e.target.value)}
                      className="w-28 shrink-0 rounded border border-kiviBorder bg-kiviSurface px-2 py-0.5 font-mono text-kiviInk"
                    >
                      <option value="gasoline">Gasoline</option>
                      <option value="e10">E10</option>
                      <option value="e85">E85</option>
                      <option value="methanol">Methanol</option>
                      <option value="diesel">Diesel</option>
                    </select>
                  </label>
                </div>
                <button
                  type="button"
                  onClick={() => void handleFuel()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Compute pulse width
                </button>
                <ResultBlock
                  state={fuelState}
                  render={(d) => (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                      <span className="whitespace-nowrap">gross {d.gross_ms.toFixed(3)} ms</span>
                      <span className="whitespace-nowrap">effective {d.effective_ms.toFixed(3)} ms</span>
                      <span className="whitespace-nowrap">dead time {d.dead_time_ms.toFixed(3)} ms</span>
                      <span className="whitespace-nowrap">AFR {d.stoich_afr.toFixed(2)}</span>
                    </div>
                  )}
                />
              </section>
            )}

            {tab === 'air' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="Displacement (cc)" value={disp} onChange={setDisp} ariaLabel="Air displacement" />
                  <Field label="Cylinders" value={cyls} onChange={setCyls} ariaLabel="Air cylinders" />
                  <Field label="MAP (kPa)" value={mapKpa} onChange={setMapKpa} ariaLabel="Air MAP" />
                  <Field label="VE ratio" value={ve} onChange={setVe} ariaLabel="Air VE" />
                  <Field label="IAT (°C)" value={iat} onChange={setIat} ariaLabel="Air IAT" />
                  <Field label="ECT (°C)" value={ect} onChange={setEct} ariaLabel="Air ECT" />
                  <Field label="RPM" value={rpm} onChange={setRpm} ariaLabel="Air RPM" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleAir()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Estimate air mass
                </button>
                <ResultBlock
                  state={airState}
                  render={(d) => (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                      <span className="whitespace-nowrap">{d.air_mass_g.toFixed(4)} g/cyl</span>
                      <span className="whitespace-nowrap">charge {d.charge_temp_k.toFixed(1)} K</span>
                      <span className="whitespace-nowrap">implied VE {(d.implied_ve * 100).toFixed(1)}%</span>
                    </div>
                  )}
                />
              </section>
            )}

            {tab === 'timing' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="Base BTDC" value={baseBtdc} onChange={setBaseBtdc} ariaLabel="Timing base" />
                  <Field label="ECT correction" value={ectCorr} onChange={setEctCorr} ariaLabel="Timing ECT correction" />
                  <Field label="IAT correction" value={iatCorr} onChange={setIatCorr} ariaLabel="Timing IAT correction" />
                  <Field label="Knock retard" value={knock} onChange={setKnock} ariaLabel="Timing knock retard" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleTiming()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Compute timing
                </button>
                <ResultBlock
                  state={timingState}
                  render={(d) => (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                      <span className="whitespace-nowrap">commanded {d.commanded_btdc.toFixed(1)}° BTDC</span>
                      <span className="whitespace-nowrap">retard {d.knock_retard_deg.toFixed(1)}°</span>
                      <span className="whitespace-nowrap">MFB50 {d.mfb50_atdc.toFixed(1)}° ATDC</span>
                    </div>
                  )}
                />
              </section>
            )}

            {tab === 'torque' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="Pedal (%)" value={pedal} onChange={setPedal} ariaLabel="Torque pedal" />
                  <Field label="RPM" value={tqRpm} onChange={setTqRpm} ariaLabel="Torque RPM" />
                  <Field label="MAP (kPa)" value={tqMap} onChange={setTqMap} ariaLabel="Torque MAP" />
                  <Field label="ECT (°C)" value={tqEct} onChange={setTqEct} ariaLabel="Torque ECT" />
                  <Field label="Accessory (Nm)" value={accessory} onChange={setAccessory} ariaLabel="Torque accessory" />
                  <Field label="Target (Nm)" value={targetTq} onChange={setTargetTq} ariaLabel="Torque target" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleTorque()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Compute torque
                </button>
                <ResultBlock
                  state={torqueState}
                  render={(d) => (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                      <span className="whitespace-nowrap">driver {d.driver_torque_nm.toFixed(1)} Nm</span>
                      <span className="whitespace-nowrap">friction {d.friction_nm.toFixed(1)} Nm</span>
                      <span className="whitespace-nowrap">pumping {d.pumping_nm.toFixed(1)} Nm</span>
                      <span className="whitespace-nowrap">indicated {d.indicated_nm.toFixed(1)} Nm</span>
                      <span className="whitespace-nowrap">fast-path {d.fast_path_retard_deg.toFixed(1)}°</span>
                    </div>
                  )}
                />
              </section>
            )}

            {tab === 'sensor' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <label className="flex items-center justify-between gap-3 rounded border border-kiviBorder/70 bg-kiviBg/40 px-2.5 py-1.5 text-[11px] text-kiviMuted">
                    <span className="whitespace-nowrap">Sensor</span>
                    <select
                      aria-label="Sensor kind"
                      value={sensorKind}
                      onChange={(e) => setSensorKind(e.target.value)}
                      className="w-28 shrink-0 rounded border border-kiviBorder bg-kiviSurface px-2 py-0.5 font-mono text-kiviInk"
                    >
                      <option value="map">MAP (volts)</option>
                      <option value="tps">TPS (ADC)</option>
                      <option value="thermistor">Thermistor (ADC)</option>
                    </select>
                  </label>
                  <Field label="Raw value" value={sensorRaw} onChange={setSensorRaw} ariaLabel="Sensor raw" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleSensor()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Decode
                </button>
                <ResultBlock
                  state={sensorState}
                  render={(d) => (
                    <div className="font-mono whitespace-nowrap" aria-live="polite">
                      {d.value.toFixed(2)} {d.unit}
                    </div>
                  )}
                />
              </section>
            )}

            {tab === 'histogram' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="Row axis" value={histRows} onChange={setHistRows} ariaLabel="Histogram row axis" />
                  <Field label="Col axis" value={histCols} onChange={setHistCols} ariaLabel="Histogram col axis" />
                </div>
                <label className="flex flex-col gap-1.5 text-[11px] text-kiviMuted">
                  <span className="whitespace-nowrap">Operating points (row,col per line)</span>
                  <textarea
                    aria-label="Histogram operating points"
                    value={histPoints}
                    onChange={(e) => setHistPoints(e.target.value)}
                    rows={5}
                    className="w-full rounded border border-kiviBorder bg-kiviBg px-2.5 py-1.5 font-mono text-kiviInk"
                  />
                </label>
                <button
                  type="button"
                  onClick={() => void handleHistogram()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Build hit map
                </button>
                <ResultBlock
                  state={histState}
                  render={(d) => {
                    const max = d.cells.reduce((m, c) => Math.max(m, c.hits), 0) || 1;
                    return (
                      <div className="space-y-2" aria-live="polite">
                        <p className="font-mono whitespace-nowrap">
                          {d.total_samples} samples across {d.cells.length} occupied cell
                          {d.cells.length === 1 ? '' : 's'}
                        </p>
                        <div className="max-h-40 space-y-1 overflow-y-auto">
                          {d.cells.map((c) => (
                            <div key={`${c.row}-${c.col}`} className="flex items-center gap-2 font-mono text-[10px]">
                              <span className="w-16 shrink-0 whitespace-nowrap text-kiviMuted">
                                r{c.row} c{c.col}
                              </span>
                              <span
                                className="inline-block h-2 rounded-sm bg-kiviAccent"
                                style={{ width: `${Math.max(4, (c.hits / max) * 120)}px` }}
                              />
                              <span className="whitespace-nowrap text-kiviSecondary">{c.hits}</span>
                            </div>
                          ))}
                        </div>
                      </div>
                    );
                  }}
                />
              </section>
            )}

            {tab === 'chemistry' && (
              <section className="space-y-4">
                <div className="space-y-2.5">
                  <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                    <Field label="Ethanol (%)" value={ethanol} onChange={setEthanol} ariaLabel="Blend ethanol" />
                  </div>
                  <button
                    type="button"
                    onClick={() => void handleBlend()}
                    className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                  >
                    Compute blend
                  </button>
                  <ResultBlock
                    state={chemState}
                    render={(d) => (
                      <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                        <span className="whitespace-nowrap">AFR {d.stoich_afr.toFixed(2)}</span>
                        <span className="whitespace-nowrap">volume x{d.volume_multiplier.toFixed(3)}</span>
                        <span className="whitespace-nowrap">latent {d.latent_heat_kj_kg.toFixed(0)} kJ/kg</span>
                      </div>
                    )}
                  />
                </div>

                <div className="space-y-2.5 border-t border-kiviBorder pt-3">
                  <p className="whitespace-nowrap text-[10px] font-semibold uppercase tracking-wide text-kiviMuted">
                    Turbo compressor
                  </p>
                  <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                    <Field label="Inlet (°C)" value={compTIn} onChange={setCompTIn} ariaLabel="Compressor inlet" />
                    <Field label="Pressure ratio" value={compPr} onChange={setCompPr} ariaLabel="Compressor pressure ratio" />
                    <Field label="Isentropic η" value={compEta} onChange={setCompEta} ariaLabel="Compressor efficiency" />
                    <Field label="Intercooler η" value={compIc} onChange={setCompIc} ariaLabel="Intercooler efficiency" />
                    <Field label="Ambient (°C)" value={compAmb} onChange={setCompAmb} ariaLabel="Compressor ambient" />
                    <Field label="Lambda" value={compLambda} onChange={setCompLambda} ariaLabel="Compressor lambda" />
                    <Field label="Retard from MBT" value={compRetard} onChange={setCompRetard} ariaLabel="Compressor retard" />
                  </div>
                  <button
                    type="button"
                    onClick={() => void handleCompressor()}
                    className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                  >
                    Compute compressor
                  </button>
                  <ResultBlock
                    state={compState}
                    render={(d) => (
                      <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                        <span className="whitespace-nowrap">discharge {d.discharge_c.toFixed(1)}°C</span>
                        <span className="whitespace-nowrap">intercooler out {d.intercooler_out_c.toFixed(1)}°C</span>
                        <span className="whitespace-nowrap">EGT {d.egt_c.toFixed(0)}°C</span>
                      </div>
                    )}
                  />
                </div>
              </section>
            )}

            {tab === 'transient' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="RPM" value={trRpm} onChange={setTrRpm} ariaLabel="Transient rpm" />
                  <Field label="TPS (%)" value={trTps} onChange={setTrTps} ariaLabel="Transient tps" />
                  <Field label="Speed (kph)" value={trSpeed} onChange={setTrSpeed} ariaLabel="Transient speed" />
                  <Field label="ECT (°C)" value={trEct} onChange={setTrEct} ariaLabel="Transient ect" />
                  <Field label="Target idle" value={trIdle} onChange={setTrIdle} ariaLabel="Transient target idle" />
                  <Field label="MAF (V)" value={trMaf} onChange={setTrMaf} ariaLabel="Transient maf" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleTransient()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Evaluate transient
                </button>
                <ResultBlock
                  state={trState}
                  render={(d) => (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                      <span className="whitespace-nowrap">DFCO x{d.dfco_multiplier.toFixed(2)}</span>
                      <span className="whitespace-nowrap">anti-stall +{d.anti_stall_adder_deg.toFixed(1)}°</span>
                      <span className="whitespace-nowrap">MAF {d.maf_airflow_gps.toFixed(2)} g/s</span>
                    </div>
                  )}
                />
              </section>
            )}

            {tab === 'rescale' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="Old PW (ms)" value={rsPw} onChange={setRsPw} ariaLabel="Rescale old pulse width" />
                  <Field label="Old dead (ms)" value={rsDeadOld} onChange={setRsDeadOld} ariaLabel="Rescale old dead time" />
                  <Field label="Old flow (cc)" value={rsFlowOld} onChange={setRsFlowOld} ariaLabel="Rescale old flow" />
                  <Field label="New flow (cc)" value={rsFlowNew} onChange={setRsFlowNew} ariaLabel="Rescale new flow" />
                  <Field label="New dead (ms)" value={rsDeadNew} onChange={setRsDeadNew} ariaLabel="Rescale new dead time" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleRescale()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Rescale injector
                </button>
                <ResultBlock
                  state={rsState}
                  render={(d) => (
                    <div className="font-mono whitespace-nowrap" aria-live="polite">
                      new pulse width {d.new_pulse_width_ms.toFixed(3)} ms
                    </div>
                  )}
                />
              </section>
            )}

            {tab === 'dtc' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="DTC code" value={dtcText} onChange={setDtcText} ariaLabel="DTC code" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleDtc()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Parse DTC
                </button>
                <ResultBlock
                  state={dtcState}
                  render={(d) => (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                      <span className="whitespace-nowrap">{d.code}</span>
                      <span className="whitespace-nowrap">category {d.category}</span>
                      <span className="whitespace-nowrap">{d.is_standard ? 'SAE standard' : 'manufacturer'}</span>
                    </div>
                  )}
                />
                <p className="text-[10px] leading-relaxed text-kiviMuted">
                  Parsing only. Patching a DTC switch is available through the command layer but
                  is not wired to a button here, because it modifies ROM bytes.
                </p>
              </section>
            )}

            {tab === 'airflow' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="Throttle (°)" value={tfAngle} onChange={setTfAngle} ariaLabel="Throttle angle" />
                  <Field label="RPM" value={tfRpm} onChange={setTfRpm} ariaLabel="Throttle rpm" />
                  <Field label="MAP (kPa)" value={tfMap} onChange={setTfMap} ariaLabel="Throttle map" />
                  <Field label="IAT (°C)" value={tfIat} onChange={setTfIat} ariaLabel="Throttle iat" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleThrottleFlow()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Compute throttle flow
                </button>
                <ResultBlock
                  state={tfState}
                  render={(d) => (
                    <div className="flex flex-wrap gap-x-4 gap-y-1 font-mono" aria-live="polite">
                      <span className="whitespace-nowrap">{d.mass_flow_kg_s.toFixed(4)} kg/s</span>
                      <span className="whitespace-nowrap">ratio {d.pressure_ratio.toFixed(3)}</span>
                      <span className="whitespace-nowrap">
                        {d.is_choked ? 'choked' : 'unchoked'} (critical {d.critical_ratio.toFixed(3)})
                      </span>
                    </div>
                  )}
                />
                <p className="text-[10px] leading-relaxed text-kiviMuted">
                  Assumes a 50 mm bore butterfly; effective area scales with sin(angle).
                </p>
              </section>
            )}

            {tab === 'chip' && (
              <section className="space-y-3">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(215px,1fr))] gap-2">
                  <Field label="JEDEC id" value={chipId} onChange={setChipId} ariaLabel="Chip id" />
                </div>
                <button
                  type="button"
                  onClick={() => void handleChip()}
                  className="whitespace-nowrap rounded bg-kiviAccent px-3 py-1.5 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90"
                >
                  Describe chip
                </button>
                <ResultBlock
                  state={chipState}
                  render={(d) => (
                    <div className="space-y-2">
                      <p className="font-mono" aria-live="polite">
                        {describeFlashTarget(d)}
                      </p>
                      {hasBlockSizeMismatch(d) && (
                        <p className="flex items-start gap-1.5 text-[11px] text-kiviWarn">
                          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                          <span>
                            Declared size and block coverage disagree. Preserved from the source
                            table; do not assume the gap is usable flash.
                          </span>
                        </p>
                      )}
                      <div className="max-h-48 space-y-1 overflow-y-auto pt-1">
                        {d.blocks.map((b) => (
                          <div
                            key={`${b.start_address}-${b.length}`}
                            className="flex items-center gap-3 font-mono text-[10px] text-kiviSecondary"
                          >
                            <Cpu className="h-3 w-3 shrink-0 text-kiviMuted" />
                            <span className="whitespace-nowrap">
                              0x{b.start_address.toString(16).toUpperCase().padStart(6, '0')}
                            </span>
                            <span className="whitespace-nowrap">{b.length} B</span>
                            <span className="whitespace-nowrap text-kiviMuted">{b.block_type}</span>
                            {!b.write_safe && (
                              <span className="whitespace-nowrap rounded border border-kiviWarn/40 bg-kiviWarn/10 px-1.5 py-0.2 text-kiviWarn">
                                write-blocked
                              </span>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                />
              </section>
            )}
      </ToolkitResponsiveLayout>
    </FloatingWindow>
  );
};
