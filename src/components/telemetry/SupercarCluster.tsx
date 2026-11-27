import React, { useId, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  BatteryCharging,
  BatteryWarning,
  Clock,
  Droplets,
  Flame,
  Gauge,
  ShieldAlert,
  Snowflake,
  Thermometer,
  WifiOff,
  Wind,
  Zap,
} from 'lucide-react';
import type { TelemetryFrame } from '../../lib/tauriBridge';
import { useFloatingWindowContext } from '../layout/FloatingWindow';
import {
  CLUSTER_MAX_RPM,
  CLUSTER_REDLINE_RPM,
  OUTER_BEZEL_ARC_PATH,
  classifyAfr,
  computeAfrMarkerPercent,
  computeFlankArcPath,
  computeFlankTrackPath,
  computeShiftLights,
  computeTachScaleLabels,
  computeTachSegments,
  formatGearLabel,
  type SemanticZone,
} from './supercarClusterModel';

export interface SupercarClusterProps {
  latestFrame: TelemetryFrame | null;
  history: TelemetryFrame[];
}

type ClusterMode = 'cluster' | 'track';

const ZONE_TOKEN_COLOR: Record<SemanticZone, string> = {
  ok: 'var(--kivi-ok)',
  accent: 'var(--kivi-accent)',
  warn: 'var(--kivi-warn)',
  danger: 'var(--kivi-danger)',
};

const TACH_SCALE_LABELS = computeTachScaleLabels();
const LEFT_FLANK_TRACK = computeFlankTrackPath('left');
const RIGHT_FLANK_TRACK = computeFlankTrackPath('right');

/**
 * Ferrari SF90 / Lamborghini Revuelto / BMW M inspired supercar telemetry cluster.
 * Renders cleanly across dark/light semantic tokens and adapts dynamically between
 * compact `380px` floating drawers and wide panoramic standalone desktop windows.
 */
export const SupercarCluster: React.FC<SupercarClusterProps> = ({ latestFrame, history }) => {
  const { width } = useFloatingWindowContext();
  const [mode, setMode] = useState<ClusterMode>('cluster');
  const scopeGradId = useId().replace(/:/g, '');

  const isWideCockpit = width >= 820;
  const isCompact = width < 440;

  const rpm = latestFrame?.rpm ?? 0;
  const speed = latestFrame?.speed_kph ?? 0;
  const tps = latestFrame?.tps ?? 0;
  const afr = latestFrame?.afr ?? 14.7;
  const mapKpa = latestFrame?.map_kpa ?? 0;
  const volts = latestFrame?.battery_volts ?? 0;
  const ect = latestFrame?.ect_c ?? 0;
  const iat = latestFrame?.iat_c ?? 0;
  const ignDeg = latestFrame?.ignition_advance_deg ?? 0;
  const pwMs = latestFrame?.injector_pulse_width_ms ?? 0;

  const gearText = formatGearLabel(latestFrame?.gear);
  const { segments: shiftLights, isShiftAlert } = computeShiftLights(rpm);
  const tachSegments = computeTachSegments(rpm);
  const rpmPercent = Math.min(100, Math.max(0, (rpm / CLUSTER_MAX_RPM) * 100));
  const tpsPercent = Math.min(100, Math.max(0, tps));
  const mapPercent = Math.min(100, Math.max(0, (mapKpa / 250) * 100));
  const afrMarkerPct = computeAfrMarkerPercent(afr);
  const afrStatus = latestFrame ? classifyAfr(afr) : null;

  const leftFlankActive = computeFlankArcPath(tpsPercent, 'left');
  const rightFlankActive = computeFlankArcPath(mapPercent, 'right');

  const isLowVoltage = Boolean(latestFrame && volts < 11.8);
  const isOverheating = Boolean(latestFrame && ect >= 105);
  const isBoost = Boolean(latestFrame && mapKpa > 105);

  const peakRpm = history.reduce((acc, f) => Math.max(acc, f.rpm), rpm);

  const sparklineRpmPoints =
    history.length >= 2
      ? history
          .map((f, i) => {
            const x = (i / Math.max(history.length - 1, 1)) * 480;
            const clamped = Math.min(CLUSTER_MAX_RPM, Math.max(0, f.rpm));
            const y = 44 - (clamped / CLUSTER_MAX_RPM) * 40;
            return `${x.toFixed(1)},${y.toFixed(1)}`;
          })
          .join(' ')
      : '';

  const sparklineAreaPoints = sparklineRpmPoints
    ? `0,44 ${sparklineRpmPoints} 480,44`
    : '';

  const sparklineAfrPoints =
    history.length >= 2
      ? history
          .map((f, i) => {
            const x = (i / Math.max(history.length - 1, 1)) * 480;
            const clamped = Math.min(20, Math.max(10, f.afr));
            const y = 44 - ((clamped - 10) / 10) * 40;
            return `${x.toFixed(1)},${y.toFixed(1)}`;
          })
          .join(' ')
      : '';

  const annunciators = [
    {
      label: 'CEL (MIL)',
      active: Boolean(latestFrame?.status_flags.check_engine_light),
      tone: 'warn' as const,
      Icon: AlertTriangle,
    },
    {
      label: 'LIMP MODE',
      active: Boolean(latestFrame?.status_flags.limp_mode),
      tone: 'warn' as const,
      Icon: ShieldAlert,
    },
    {
      label: 'LOW VOLT',
      active: Boolean(latestFrame?.status_flags.low_voltage || isLowVoltage),
      tone: 'danger' as const,
      Icon: BatteryWarning,
    },
    {
      label: 'OVERHEAT',
      active: Boolean(latestFrame?.status_flags.overheating || isOverheating),
      tone: 'danger' as const,
      Icon: Flame,
    },
    {
      label: 'STALE',
      active: Boolean(latestFrame?.status_flags.stale_sample),
      tone: 'warn' as const,
      Icon: Clock,
    },
    {
      label: 'COMM FAULT',
      active: Boolean(latestFrame?.status_flags.communication_fault),
      tone: 'danger' as const,
      Icon: WifiOff,
    },
  ];

  const renderHeroGauge = () => (
    <div
      data-testid="supercar-hero-gauge"
      className="relative flex flex-col items-center justify-between rounded-xl border border-kiviBorderStrong/80 bg-kiviSurface px-3 py-2.5"
    >
      {/* Top Instrument Mode & Shift Alert Row */}
      <div className="flex w-full items-center justify-between gap-2 pb-2">
        <div className="flex items-center gap-1.5 min-w-0">
          <Gauge className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          <span className="truncate text-[10px] font-semibold uppercase tracking-widest text-kiviSecondary">
            Engine Speed (RPM)
          </span>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          <span
            aria-hidden={!isShiftAlert}
            className={`inline-flex h-5 w-[54px] shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded border px-1.5 text-[9px] font-bold tracking-wider transition-opacity ${
              isShiftAlert
                ? 'opacity-100 border-kiviRed/60 bg-kiviRed/15 text-kiviRed'
                : 'pointer-events-none opacity-0 border-transparent bg-transparent text-transparent'
            }`}
          >
            <Zap className="h-2.5 w-2.5 shrink-0" />
            <span>SHIFT</span>
          </span>
          <div
            role="group"
            aria-label="Cluster display mode"
            className="flex rounded border border-kiviBorder bg-kiviBg p-0.5 text-[9px] font-semibold"
          >
            <button
              type="button"
              onClick={() => setMode('cluster')}
              className={`rounded px-1.5 py-0.5 transition-colors ${
                mode === 'cluster'
                  ? 'bg-kiviAccent text-kiviAccentFg'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              ARC
            </button>
            <button
              type="button"
              onClick={() => setMode('track')}
              className={`rounded px-1.5 py-0.5 transition-colors ${
                mode === 'track'
                  ? 'bg-kiviAccent text-kiviAccentFg'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              TRACK
            </button>
          </div>
        </div>
      </div>

      {/* BMW M / Ferrari Sequential LED Shift Light Bar */}
      <div
        data-testid="shift-light-bar"
        aria-label="Sequential shift light indicator"
        className="mb-2 grid w-full grid-cols-16 gap-1 px-1"
        style={{ gridTemplateColumns: `repeat(${shiftLights.length}, minmax(0, 1fr))` }}
      >
        {shiftLights.map((led) => {
          const activeClass =
            led.zone === 'ok'
              ? 'bg-kiviGreen border-kiviGreen/70'
              : led.zone === 'warn'
                ? 'bg-kiviWarn border-kiviWarn/70'
                : 'bg-kiviRed border-kiviRed/80';
          return (
            <div
              key={led.index}
              title={`${led.thresholdRpm} RPM`}
              className={`h-2 -skew-x-12 rounded-[2px] border transition-colors ${
                led.active
                  ? `${activeClass} ${isShiftAlert && led.zone === 'danger' ? 'animate-pulse' : ''}`
                  : 'border-kiviBorder/60 bg-kiviElevated/70'
              }`}
            />
          );
        })}
      </div>

      {mode === 'cluster' ? (
        <div className="relative w-full max-w-[360px]">
          <svg
            viewBox="0 0 360 226"
            className="w-full h-auto select-none"
            aria-label="Digital tachometer arc gauge"
          >
            {/* Open structural bezel horseshoe (-215°..+35°), zero bottom bleed */}
            <path d={OUTER_BEZEL_ARC_PATH} fill="var(--kivi-border)" fillOpacity="0.6" />

            {/* Mirrored Flank Wings: TPS (Left) & MAP Boost (Right) */}
            <path d={LEFT_FLANK_TRACK} fill="var(--kivi-elevated)" fillOpacity="0.9" />
            {leftFlankActive && <path d={leftFlankActive} fill="var(--kivi-accent)" />}

            <path d={RIGHT_FLANK_TRACK} fill="var(--kivi-elevated)" fillOpacity="0.9" />
            {rightFlankActive && (
              <path
                d={rightFlankActive}
                fill={isBoost ? 'var(--kivi-warn)' : 'var(--kivi-ok)'}
              />
            )}

            <text
              x="46"
              y="196"
              textAnchor="middle"
              fill="var(--kivi-muted)"
              fontSize="8.5"
              fontWeight="600"
            >
              TPS
            </text>
            <text
              x="314"
              y="196"
              textAnchor="middle"
              fill="var(--kivi-muted)"
              fontSize="8.5"
              fontWeight="600"
            >
              MAP
            </text>

            {/* 36 Angular Rev Segments */}
            {tachSegments.map((seg) => (
              <path
                key={seg.index}
                d={seg.path}
                fill={seg.active ? ZONE_TOKEN_COLOR[seg.zone] : 'var(--kivi-elevated)'}
                fillOpacity={seg.active ? 0.96 : seg.zone === 'danger' ? 0.45 : 0.72}
              />
            ))}

            {/* Numbered RPM Scale */}
            {TACH_SCALE_LABELS.map((tick) => (
              <text
                key={tick.rpmThousands}
                x={tick.x}
                y={tick.y + 3}
                textAnchor="middle"
                fill={tick.isRedline ? 'var(--kivi-danger)' : 'var(--kivi-muted)'}
                fontSize="10"
                fontWeight={tick.isRedline ? '700' : '600'}
              >
                {tick.rpmThousands}
              </text>
            ))}

            {/* Center Telemetry Hub */}
            <text
              x="180"
              y="102"
              textAnchor="middle"
              fill="var(--kivi-muted)"
              fontSize="8.5"
              letterSpacing="1.6"
            >
              x1000 RPM
            </text>
            <text
              x="180"
              y="131"
              textAnchor="middle"
              fill={isShiftAlert ? 'var(--kivi-danger)' : 'var(--kivi-ink)'}
              fontSize="30"
              fontWeight="700"
              style={{ fontVariantNumeric: 'tabular-nums' }}
            >
              {Math.round(rpm).toLocaleString()}
            </text>

            {/* Lower-Center Gear Core Box + Lower Cockpit Speed & Throttle Pods */}
            <polygon
              points="154,145 206,145 213,158 206,186 154,186 147,158"
              fill="var(--kivi-bg)"
              stroke="var(--kivi-accent)"
              strokeOpacity="0.65"
              strokeWidth="1.25"
            />
            <text
              x="180"
              y="156"
              textAnchor="middle"
              fill="var(--kivi-muted)"
              fontSize="7.5"
              letterSpacing="1.2"
            >
              GEAR
            </text>
            <text
              x="180"
              y="179"
              textAnchor="middle"
              fill="var(--kivi-accent)"
              fontSize="20"
              fontWeight="700"
              style={{ fontVariantNumeric: 'tabular-nums' }}
            >
              {gearText}
            </text>

            <rect
              x="88"
              y="193"
              width="66"
              height="28"
              rx="5"
              fill="var(--kivi-bg)"
              stroke="var(--kivi-border)"
              strokeOpacity="0.8"
              strokeWidth="1"
            />
            <text
              x="121"
              y="203"
              textAnchor="middle"
              fill="var(--kivi-muted)"
              fontSize="7"
              letterSpacing="1"
            >
              KM/H
            </text>
            <text
              x="121"
              y="216"
              textAnchor="middle"
              fill="var(--kivi-ink)"
              fontSize="12.5"
              fontWeight="700"
              style={{ fontVariantNumeric: 'tabular-nums' }}
            >
              {latestFrame ? Math.round(speed) : '--'}
            </text>

            <rect
              x="206"
              y="193"
              width="66"
              height="28"
              rx="5"
              fill="var(--kivi-bg)"
              stroke="var(--kivi-border)"
              strokeOpacity="0.8"
              strokeWidth="1"
            />
            <text
              x="239"
              y="203"
              textAnchor="middle"
              fill="var(--kivi-muted)"
              fontSize="7"
              letterSpacing="1"
            >
              THROTTLE
            </text>
            <text
              x="239"
              y="216"
              textAnchor="middle"
              fill="var(--kivi-ink)"
              fontSize="12.5"
              fontWeight="700"
              style={{ fontVariantNumeric: 'tabular-nums' }}
            >
              {latestFrame ? `${tps.toFixed(0)}%` : '--'}
            </text>
          </svg>
        </div>
      ) : (
        /* TRACK LINEAR MODE (BMW M HUD / Corsa Strip) */
        <div className="flex min-h-[156px] w-full flex-col justify-between gap-2.5 py-1">
          <div className="flex min-w-0 items-center justify-between gap-2">
            <div className="min-w-0">
              <div className="truncate text-[10px] uppercase tracking-widest text-kiviMuted">
                Track Tachometer
              </div>
              <div className="flex items-baseline gap-1.5">
                <span
                  className={`inline-block min-w-[6ch] whitespace-nowrap font-bold tabular-nums ${
                    isCompact ? 'text-2xl' : 'text-3xl'
                  } ${isShiftAlert ? 'text-kiviRed' : 'text-kiviInk'}`}
                >
                  {Math.round(rpm).toLocaleString()}
                </span>
                <span className="shrink-0 text-xs text-kiviMuted">RPM</span>
              </div>
            </div>

            <div className="flex shrink-0 items-center gap-2">
              <div className="w-[58px] shrink-0 whitespace-nowrap rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-center tabular-nums">
                <div className="text-[9px] text-kiviMuted">KM/H</div>
                <div className="text-base font-bold tabular-nums text-kiviInk">
                  {latestFrame ? Math.round(speed) : '--'}
                </div>
              </div>
              <div className="w-[50px] shrink-0 whitespace-nowrap rounded border border-kiviAccent/50 bg-kiviAccent/10 px-2 py-1 text-center tabular-nums">
                <div className="text-[9px] text-kiviAccent">GEAR</div>
                <div className="text-base font-bold tabular-nums text-kiviAccent">{gearText}</div>
              </div>
            </div>
          </div>

          <div className="grid grid-cols-24 gap-0.5 rounded border border-kiviBorder bg-kiviBg p-1"
            style={{ gridTemplateColumns: 'repeat(24, minmax(0, 1fr))' }}
          >
            {Array.from({ length: 24 }, (_, idx) => {
              const frac = (idx + 1) / 24;
              const active = rpmPercent >= (idx / 24) * 100 && rpm > 0;
              const isRed = frac >= CLUSTER_REDLINE_RPM / CLUSTER_MAX_RPM;
              const isWarn = !isRed && frac >= 0.7;
              const bg = active
                ? isRed
                  ? 'bg-kiviRed'
                  : isWarn
                    ? 'bg-kiviWarn'
                    : 'bg-kiviAccent'
                : 'bg-kiviElevated';
              return <div key={idx} className={`h-5 rounded-[1px] ${bg}`} />;
            })}
          </div>
          <div className="flex justify-between text-[9px] text-kiviMuted tabular-nums">
            <span>0</span>
            <span>3,000</span>
            <span>6,000</span>
            <span>9,000</span>
            <span className="font-bold text-kiviRed">10,000+</span>
            <span>12,000</span>
          </div>
        </div>
      )}
    </div>
  );

  const renderLeftWingPods = () => (
    <div className={isWideCockpit ? 'grid h-full grid-rows-3 gap-2' : 'flex flex-col gap-2'}>
      <div
        data-testid="telemetry-card-tps"
        className="flex min-h-[68px] flex-col justify-between rounded-lg border border-kiviBorder bg-kiviSurface p-2.5"
      >
        <div className="flex items-center justify-between gap-2 text-[11px]">
          <span className="flex min-w-0 items-center gap-1.5 text-kiviMuted">
            <Activity className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
            <span className="truncate whitespace-nowrap">Throttle (TPS)</span>
          </span>
          <span className="w-[46px] shrink-0 text-right font-bold tabular-nums whitespace-nowrap text-kiviInk">
            {latestFrame ? `${tps.toFixed(1)}%` : '--'}
          </span>
        </div>
        <div className="h-1.5 w-full overflow-hidden rounded-full bg-kiviBg border border-kiviBorder/80">
          <div
            className="h-full bg-kiviAccent transition-all duration-75"
            style={{ width: `${tpsPercent}%` }}
          />
        </div>
      </div>

      <div
        data-testid="telemetry-card-map"
        className="flex min-h-[68px] flex-col justify-between rounded-lg border border-kiviBorder bg-kiviSurface p-2.5"
      >
        <div className="flex h-4 items-center justify-between gap-2 text-[11px] text-kiviMuted">
          <span className="flex min-w-0 items-center gap-1.5">
            <Wind className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
            <span className="truncate whitespace-nowrap">MAP Pressure</span>
          </span>
          <span
            aria-hidden={!isBoost}
            className={`inline-flex h-4 w-[46px] shrink-0 items-center justify-center rounded px-1 text-[9px] font-bold leading-none whitespace-nowrap transition-opacity ${
              isBoost ? 'opacity-100 bg-kiviWarn/15 text-kiviWarn' : 'pointer-events-none opacity-0 text-transparent'
            }`}
          >
            BOOST
          </span>
        </div>
        <div className="mt-1 flex items-baseline justify-between gap-2 whitespace-nowrap">
          <span className="text-lg font-bold tabular-nums text-kiviInk">
            {latestFrame ? mapKpa.toFixed(1) : '--'}
          </span>
          <span className="shrink-0 text-[10px] text-kiviMuted">kPa</span>
        </div>
      </div>

      <div
        data-testid="telemetry-card-pw"
        className="flex min-h-[68px] flex-col justify-between rounded-lg border border-kiviBorder bg-kiviSurface p-2.5"
      >
        <span className="flex min-w-0 items-center gap-1.5 text-[11px] text-kiviMuted">
          <Droplets className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          <span className="truncate whitespace-nowrap">Injector PW</span>
        </span>
        <div className="mt-1 flex items-baseline justify-between gap-2 whitespace-nowrap">
          <span className="text-lg font-bold tabular-nums text-kiviInk">
            {latestFrame ? pwMs.toFixed(2) : '--'}
          </span>
          <span className="shrink-0 text-[10px] text-kiviMuted">ms</span>
        </div>
      </div>
    </div>
  );

  const renderRightWingPods = () => {
    const afrBadgeTone =
      afrStatus === 'rich'
        ? 'border-kiviAccent/40 bg-kiviAccent/10 text-kiviAccent'
        : afrStatus === 'lean'
          ? 'border-kiviRed/40 bg-kiviRed/10 text-kiviRed'
          : afrStatus === 'stoich'
            ? 'border-kiviGreen/40 bg-kiviGreen/10 text-kiviGreen'
            : 'border-kiviBorder bg-kiviBg text-kiviMuted';
    const afrBadgeText =
      afrStatus === 'rich'
        ? 'RICH'
        : afrStatus === 'lean'
          ? 'LEAN'
          : afrStatus === 'stoich'
            ? 'STOICH'
            : '--';

    return (
      <div className={isWideCockpit ? 'grid h-full grid-rows-3 gap-2' : 'flex flex-col gap-2'}>
        <div
          data-testid="telemetry-card-afr"
          className="flex min-h-[68px] flex-col justify-between rounded-lg border border-kiviBorder bg-kiviSurface p-2.5"
        >
          <div className="flex h-4 items-center justify-between gap-1.5 text-[11px]">
            <span className="flex min-w-0 items-center gap-1.5 text-kiviMuted">
              <Flame className="h-3.5 w-3.5 shrink-0 text-kiviWarn" />
              <span className="truncate whitespace-nowrap">Air/Fuel</span>
            </span>
            <div className="flex shrink-0 items-center gap-1">
              <span
                className={`inline-flex h-4 w-[46px] shrink-0 items-center justify-center rounded border px-1 text-[9px] font-bold leading-none whitespace-nowrap ${afrBadgeTone}`}
              >
                {afrBadgeText}
              </span>
              <span className="w-[40px] shrink-0 text-right font-bold tabular-nums whitespace-nowrap text-kiviInk">
                {latestFrame ? afr.toFixed(2) : '--'}
              </span>
            </div>
          </div>
          <div className="relative h-1.5 w-full rounded-full bg-kiviBg border border-kiviBorder/80">
            <div
              className="absolute top-0 bottom-0 left-[44%] w-[6%] rounded-full bg-kiviGreen/40"
              aria-hidden="true"
            />
            {latestFrame && (
              <div
                className="absolute -top-0.5 h-2.5 w-1.5 -translate-x-1/2 rounded-sm bg-kiviInk"
                style={{ left: `${afrMarkerPct}%` }}
              />
            )}
          </div>
          <div className="flex justify-between text-[9px] text-kiviMuted tabular-nums">
            <span>10.0</span>
            <span>14.7</span>
            <span>20.0</span>
          </div>
        </div>

        <div
          data-testid="telemetry-card-ign"
          className="flex min-h-[68px] flex-col justify-between rounded-lg border border-kiviBorder bg-kiviSurface p-2.5"
        >
          <span className="flex min-w-0 items-center gap-1.5 text-[11px] text-kiviMuted">
            <Zap className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
            <span className="truncate whitespace-nowrap">Ignition Adv</span>
          </span>
          <div className="mt-1 flex items-baseline justify-between gap-2 whitespace-nowrap">
            <span className="text-lg font-bold tabular-nums text-kiviInk">
              {latestFrame ? ignDeg.toFixed(1) : '--'}
            </span>
            <span className="shrink-0 text-[10px] text-kiviMuted">deg BTDC</span>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div
            data-testid="telemetry-card-batt"
            className={`flex min-h-[54px] flex-col justify-between rounded-lg border p-2 ${
              isLowVoltage
                ? 'border-kiviRed/50 bg-kiviRed/10 text-kiviRed'
                : 'border-kiviBorder bg-kiviSurface text-kiviInk'
            }`}
          >
            <span className="flex items-center gap-1 text-[10px] text-kiviMuted">
              <BatteryCharging className="h-3 w-3 shrink-0 text-kiviGreen" />
              <span className="truncate whitespace-nowrap">BATT</span>
            </span>
            <div className="mt-0.5 text-sm font-bold tabular-nums whitespace-nowrap">
              {latestFrame ? `${volts.toFixed(2)}V` : '--'}
            </div>
          </div>

          <div
            data-testid="telemetry-card-ect"
            className={`flex min-h-[54px] flex-col justify-between rounded-lg border p-2 ${
              isOverheating
                ? 'border-kiviRed/50 bg-kiviRed/10 text-kiviRed'
                : 'border-kiviBorder bg-kiviSurface text-kiviInk'
            }`}
          >
            <span className="flex items-center gap-1 text-[10px] text-kiviMuted">
              <Thermometer className="h-3 w-3 shrink-0 text-kiviWarn" />
              <span className="truncate whitespace-nowrap">ECT</span>
            </span>
            <div className="mt-0.5 text-sm font-bold tabular-nums whitespace-nowrap">
              {latestFrame ? `${ect.toFixed(1)}°C` : '--'}
            </div>
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-3 font-mono">
      {isWideCockpit ? (
        <div className="grid grid-cols-[216px_minmax(340px,1fr)_216px] items-stretch gap-3">
          {renderLeftWingPods()}
          {renderHeroGauge()}
          {renderRightWingPods()}
        </div>
      ) : (
        <div className="space-y-2.5">
          {renderHeroGauge()}
          <div
            className={`grid gap-2 ${
              isCompact
                ? 'grid-cols-1'
                : 'grid-cols-[repeat(auto-fit,minmax(160px,1fr))]'
            }`}
          >
            {renderLeftWingPods()}
            {renderRightWingPods()}
          </div>
        </div>
      )}

      {/* Secondary Thermal & Peak Summary Strip */}
      <div className="grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-2">
        <div
          data-testid="telemetry-card-iat"
          className="flex min-h-[34px] items-center justify-between gap-2 rounded-lg border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 text-[11px]"
        >
          <span className="flex min-w-0 items-center gap-1.5 text-kiviMuted">
            <Snowflake className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
            <span className="truncate whitespace-nowrap">Intake (IAT)</span>
          </span>
          <span className="min-w-[54px] shrink-0 text-right font-bold tabular-nums whitespace-nowrap text-kiviInk">
            {latestFrame ? `${iat.toFixed(1)}°C` : '--'}
          </span>
        </div>

        <div
          data-testid="telemetry-card-peak-rpm"
          className="flex min-h-[34px] items-center justify-between gap-2 rounded-lg border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 text-[11px]"
        >
          <span className="flex min-w-0 items-center gap-1.5 text-kiviMuted">
            <Gauge className="h-3.5 w-3.5 shrink-0 text-kiviWarn" />
            <span className="truncate whitespace-nowrap">Peak RPM</span>
          </span>
          <span className="min-w-[54px] shrink-0 text-right font-bold tabular-nums whitespace-nowrap text-kiviInk">
            {history.length > 0 || latestFrame ? Math.round(peakRpm).toLocaleString() : '--'}
          </span>
        </div>
      </div>

      {/* Diagnostic Status Annunciator (Icon + Text always paired) */}
      <div
        data-testid="telemetry-card-annunciator"
        className="rounded-lg border border-kiviBorder bg-kiviSurface/60 p-2.5"
      >
        <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
          Diagnostic Status Annunciator
        </div>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(102px,1fr))] gap-1.5 text-[10px]">
          {annunciators.map(({ label, active, tone, Icon }) => (
            <div
              key={label}
              className={`flex h-7 items-center justify-center gap-1 whitespace-nowrap rounded border px-2 font-semibold transition-colors ${
                active
                  ? tone === 'danger'
                    ? 'border-kiviRed/50 bg-kiviRed/15 text-kiviRed'
                    : 'border-kiviWarn/50 bg-kiviWarn/15 text-kiviWarn'
                  : 'border-kiviBorder/50 bg-kiviBg text-kiviMuted'
              }`}
            >
              <Icon className="h-3 w-3 shrink-0" />
              <span className="truncate whitespace-nowrap">{label}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Oscilloscope Live Trend Buffer */}
      <div
        data-testid="telemetry-card-trend-buffer"
        className="rounded-lg border border-kiviBorder bg-kiviSurface/80 p-2.5 space-y-1.5"
      >
        <div className="flex items-center justify-between gap-2 text-[11px]">
          <span className="truncate font-medium text-kiviSecondary">
            Live Trend Buffer (Last 100 Frames)
          </span>
          <div className="flex shrink-0 items-center gap-2.5 text-[10px]">
            <span className="flex items-center gap-1 whitespace-nowrap text-kiviAccent">
              <span className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-kiviAccent" />
              RPM
            </span>
            <span className="flex items-center gap-1 whitespace-nowrap text-kiviGreen">
              <span className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-kiviGreen" />
              AFR
            </span>
            <span className="w-[80px] shrink-0 text-right tabular-nums whitespace-nowrap text-kiviMuted">
              Depth: {history.length}/100
            </span>
          </div>
        </div>

        <div className="relative h-14 w-full overflow-hidden rounded border border-kiviBorder/80 bg-kiviBg">
          <svg className="h-full w-full" viewBox="0 0 480 44" preserveAspectRatio="none">
            <defs>
              <linearGradient id={scopeGradId} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--kivi-accent)" stopOpacity="0.28" />
                <stop offset="100%" stopColor="var(--kivi-accent)" stopOpacity="0.02" />
              </linearGradient>
            </defs>
            <line
              x1="0"
              y1="22"
              x2="480"
              y2="22"
              stroke="var(--kivi-border)"
              strokeDasharray="3 3"
              strokeWidth="0.6"
            />
            {!sparklineRpmPoints && (
              <text
                x="240"
                y="26"
                textAnchor="middle"
                fill="var(--kivi-muted)"
                fontSize="10"
              >
                Awaiting telemetry stream...
              </text>
            )}
            {sparklineAreaPoints && <polygon fill={`url(#${scopeGradId})`} points={sparklineAreaPoints} />}
            {sparklineRpmPoints && (
              <polyline
                fill="none"
                stroke="var(--kivi-accent)"
                strokeWidth="1.6"
                strokeLinecap="round"
                strokeLinejoin="round"
                points={sparklineRpmPoints}
              />
            )}
            {sparklineAfrPoints && (
              <polyline
                fill="none"
                stroke="var(--kivi-ok)"
                strokeWidth="1.1"
                strokeDasharray="2.5 2.5"
                points={sparklineAfrPoints}
              />
            )}
          </svg>
        </div>
      </div>
    </div>
  );
};
