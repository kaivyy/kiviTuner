import React, { useState } from 'react';
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Cpu,
  Loader2,
  RefreshCw,
  ShieldCheck,
  Usb,
  Wand2,
} from 'lucide-react';
import {
  describeDeviceRisk,
  describeDriverUsability,
  describeTransportCapability,
  ELM_BUS_PROFILE_OPTIONS,
  type DiscoveredDriverView,
  type ElmBusProfile,
  type ElmCapabilityReport,
  type FlashDeviceView,
  type FlashState,
  type SerialPortView,
  type TransportChoice,
} from './flashController';

export interface FlashTransportSectionProps {
  isBusy: boolean;
  autoConnecting: boolean;
  transport: TransportChoice | null;
  transportError: string | null;
  deviceState: FlashState<FlashDeviceView[]> | null;
  activeDevice: FlashDeviceView | null;
  drivers: FlashState<DiscoveredDriverView[]> | null;
  serialPorts: SerialPortView[];
  serialPort: string;
  onChangeSerialPort: (val: string) => void;
  elmBaudRate: number | undefined;
  onChangeElmBaudRate: (val: number | undefined) => void;
  elmBusProfile: ElmBusProfile;
  onChangeElmBusProfile: (val: ElmBusProfile) => void;
  elmWifiEndpoint: string;
  onChangeElmWifiEndpoint: (val: string) => void;
  klineBaudRate: number;
  onChangeKlineBaudRate: (val: number) => void;
  elmCapabilities: FlashState<ElmCapabilityReport> | null;
  onProbeElmCapabilities: () => void;
  doipHost: string;
  onChangeDoipHost: (val: string) => void;
  canIface: string;
  onChangeCanIface: (val: string) => void;
  onAutoConnect: () => void;
  onRefresh: () => void;
  onDetach: () => void;
  onAttachOpenPort2: (port?: string) => void;
  onAttachSerial: (port?: string) => void;
  onAttachElmWifi: () => void;
  onAttachKLineSerial: (port?: string) => void;
  onReconfigureTransport: (
    protocol: string,
    baudRate: number,
    txCanIdHex: string,
    rxCanIdHex: string,
    klineNoChecksum: boolean
  ) => void;
  onAttachJ2534: (path: string) => void;
  onAttachDoIp: () => void;
  onAttachSocketCan: () => void;
}

function StateLine<T>({
  state,
  render,
}: {
  state: FlashState<T> | null;
  render: (d: T) => React.ReactNode;
}) {
  if (!state) return <p className="text-[11px] text-kiviMuted">No hardware check run yet.</p>;
  if (state.kind === 'no-backend') return <p className="text-[11px] text-kiviWarn">{state.message}</p>;
  if (state.kind === 'error') return <p className="text-[11px] text-kiviRed">{state.message}</p>;
  return <div className="text-[11px] text-kiviInk">{render(state.data)}</div>;
}

function portBadgeLabel(p: SerialPortView): string {
  if (p.is_openport2) return 'OpenPort 2.0 USB-CDC';
  if (p.adapter_hint === 'tactrix_clone_candidate') return 'USB-UART / Tactrix / KKL';
  if (p.adapter_hint === 'bluetooth_spp') return 'Bluetooth SPP (ELM/OBD)';
  if (p.adapter_hint === 'elm327_candidate') return 'ELM327 / OBD';
  return p.description || 'Serial Port';
}

export const FlashTransportSection: React.FC<FlashTransportSectionProps> = ({
  isBusy,
  autoConnecting,
  transport,
  transportError,
  deviceState,
  activeDevice,
  drivers,
  serialPorts,
  serialPort,
  onChangeSerialPort,
  elmBaudRate,
  onChangeElmBaudRate,
  elmBusProfile,
  onChangeElmBusProfile,
  elmWifiEndpoint,
  onChangeElmWifiEndpoint,
  klineBaudRate,
  onChangeKlineBaudRate,
  elmCapabilities,
  onProbeElmCapabilities,
  doipHost,
  onChangeDoipHost,
  canIface,
  onChangeCanIface,
  onAutoConnect,
  onRefresh,
  onDetach,
  onAttachOpenPort2,
  onAttachSerial,
  onAttachElmWifi,
  onAttachKLineSerial,
  onReconfigureTransport,
  onAttachJ2534,
  onAttachDoIp,
  onAttachSocketCan,
}) => {
  const [manualOpen, setManualOpen] = useState(false);
  const [customLibPath, setCustomLibPath] = useState('');
  const [reconfProto, setReconfProto] = useState('iso15765');
  const [reconfBaud, setReconfBaud] = useState('500000');
  const [reconfTxId, setReconfTxId] = useState('0x7E0');
  const [reconfRxId, setReconfRxId] = useState('0x7E8');
  const [reconfKlineNoCs, setReconfKlineNoCs] = useState(false);

  const risk = activeDevice ? describeDeviceRisk(activeDevice) : null;
  const openPortCandidates = serialPorts.filter(
    (p) => p.is_openport2 || p.adapter_hint === 'tactrix_clone_candidate'
  );
  const driverList = drivers && drivers.kind === 'ok' ? drivers.data : [];
  const has32BitDllWarning = driverList.some(
    (d) => !d.loadable && (d.arch_label?.includes('32-bit') || d.reason?.includes('32-bit'))
  );
  const isElmTransport = transport === 'serial' || transport === 'elm_wifi';

  return (
    <div className="space-y-3 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
      {/* Header + Auto-Detect Primary Action -------------------------- */}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Usb className="h-3.5 w-3.5 text-kiviAccent" />
          <h3 className="text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
            Hardware Cable, Bluetooth/Wi-Fi &amp; J2534
          </h3>
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          <button
            type="button"
            disabled={isBusy || autoConnecting}
            onClick={onAutoConnect}
            className="inline-flex items-center gap-1.5 rounded border border-kiviAccent/50 bg-kiviAccent px-2.5 py-1 text-[11px] font-semibold text-kiviAccentFg hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {autoConnecting ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Wand2 className="h-3.5 w-3.5" />
            )}
            <span>
              {autoConnecting ? 'Scanning & Connecting...' : 'Auto-Detect & Connect Cable'}
            </span>
          </button>
          <button
            type="button"
            disabled={isBusy || autoConnecting}
            onClick={onRefresh}
            aria-label="Refresh hardware devices and drivers"
            className="inline-flex items-center gap-1 rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 text-[10px] text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk disabled:cursor-not-allowed disabled:opacity-50"
          >
            <RefreshCw className="h-3 w-3" />
            <span>Refresh</span>
          </button>
          {transport && (
            <button
              type="button"
              disabled={isBusy}
              onClick={onDetach}
              className="rounded border border-kiviRed/40 bg-kiviRed/10 px-2 py-1 text-[10px] text-kiviRed hover:bg-kiviRed/20 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Detach
            </button>
          )}
        </div>
      </div>

      {/* Active Transport + Device Risk ------------------------------- */}
      <div className="space-y-1.5 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-[11px] text-kiviSecondary">{describeTransportCapability(transport)}</p>
          {isElmTransport && (
            <button
              type="button"
              disabled={isBusy}
              onClick={onProbeElmCapabilities}
              className="inline-flex items-center gap-1 rounded border border-kiviAccent/40 bg-kiviAccent/10 px-2 py-0.5 text-[10px] text-kiviAccent hover:bg-kiviAccent/20 disabled:cursor-not-allowed disabled:opacity-50"
            >
              <ShieldCheck className="h-3 w-3" />
              <span>Probe Clone / AT Support</span>
            </button>
          )}
        </div>
        {transportError && <p className="text-[11px] text-kiviWarn">{transportError}</p>}
        <StateLine
          state={deviceState}
          render={(list) =>
            list.length === 0 ? (
              <span className="text-kiviMuted">
                No adapter attached. Plug USB cable (OpenPort 2.0 / J2534 / ELM327 / K-Line) or expand manual setup for Bluetooth/Wi-Fi.
              </span>
            ) : (
              <ul className="space-y-1">
                {list.map((d) => (
                  <li key={d.id} className="flex items-center justify-between">
                    <span>
                      {d.name}{' '}
                      <span className="font-mono text-kiviMuted">
                        ({d.kind} / {d.transport})
                      </span>
                    </span>
                  </li>
                ))}
              </ul>
            )
          }
        />
        {elmCapabilities?.kind === 'ok' && (
          <div className="space-y-1 rounded border border-kiviBorder bg-kiviBg/60 px-2 py-1.5 font-mono text-[10px]">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-kiviInk">
                Adapter: {elmCapabilities.data.adapter_banner} | Tier:{' '}
                <strong>{elmCapabilities.data.chip_tier}</strong>
              </span>
              <span className="text-kiviSecondary">
                ATSH:{elmCapabilities.data.supports_atsh ? 'Y' : 'N'} | CAF0:
                {elmCapabilities.data.supports_atcaf0 ? 'Y' : 'N'} | KWP5:
                {elmCapabilities.data.supports_kline_atsp5 ? 'Y' : 'N'} | ATBI:
                {elmCapabilities.data.supports_atbi ? 'Y' : 'N'}
              </span>
            </div>
            {elmCapabilities.data.warnings.map((w, idx) => (
              <p key={idx} className="text-kiviWarn">
                {w}
              </p>
            ))}
          </div>
        )}
        {elmCapabilities && elmCapabilities.kind !== 'ok' && (
          <p className="font-mono text-[10px] text-kiviWarn">{elmCapabilities.message}</p>
        )}
        {risk && (
          <div
            className={`flex items-start gap-2 rounded border px-2 py-1.5 text-[11px] ${
              activeDevice?.isMock
                ? 'border-kiviBorderStrong bg-kiviElevated text-kiviSecondary'
                : 'border-kiviRed/40 bg-kiviRed/10 text-kiviRed'
            }`}
          >
            <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
            <span>
              <strong>{risk.level}.</strong> {risk.message}
            </span>
          </div>
        )}
      </div>

      {/* Auto-Discovered Tactrix / OpenPort USB & J2534 Libraries ----- */}
      {openPortCandidates.length > 0 && (
        <div className="space-y-1.5">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-kiviMuted">
            Detected USB-CDC OpenPort / USB-UART / K-Line Ports
          </p>
          <ul className="space-y-1">
            {openPortCandidates.map((p) => (
              <li
                key={p.port_name}
                className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 text-[11px]"
              >
                <div className="min-w-0">
                  <span className="font-mono font-semibold text-kiviInk">{p.port_name}</span>{' '}
                  <span className="rounded border border-kiviAccent/40 bg-kiviAccent/10 px-1.5 py-0.5 text-[10px] text-kiviAccent">
                    {portBadgeLabel(p)}
                  </span>
                  <span className="ml-2 text-[10px] text-kiviMuted">{p.description}</span>
                </div>
                <div className="flex flex-wrap items-center gap-1.5">
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={() => onAttachOpenPort2(p.port_name)}
                    className="rounded border border-kiviAccent/40 px-2 py-0.5 text-[10px] text-kiviAccent hover:bg-kiviAccent/10"
                  >
                    Attach OpenPort 2.0
                  </button>
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={() => onAttachKLineSerial(p.port_name)}
                    className="rounded border border-kiviBorderStrong px-2 py-0.5 text-[10px] text-kiviSecondary hover:bg-kiviElevated"
                  >
                    Attach Raw K-Line
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="space-y-1.5">
        <div className="flex items-center justify-between gap-2">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-kiviMuted">
            J2534 Libraries (.dll x64/x86, Linux .so, macOS .dylib)
          </p>
        </div>
        {has32BitDllWarning && (
          <p className="rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-1.5 text-[10px] text-kiviWarn">
            32-bit x86 J2534 DLL detected. A 64-bit desktop build cannot load 32-bit DLLs directly.
            Use the 32-bit bridge or native OpenPort 2.0 USB-CDC above instead of op20pt32.dll.
          </p>
        )}
        <StateLine
          state={drivers}
          render={(list) =>
            list.length === 0 ? (
              <span className="text-kiviMuted">
                No external J2534 library found in OS registry, /usr/lib, ~/.passthru, or KIVI_J2534_DIR.
              </span>
            ) : (
              <ul className="space-y-1.5">
                {list.map((d) => (
                  <li
                    key={d.path}
                    className="flex flex-col gap-0.5 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className={`font-medium ${d.loadable ? 'text-kiviGreen' : 'text-kiviWarn'}`}>
                        <Cpu className="mr-1 inline h-3 w-3" />
                        {d.name}{' '}
                        {d.arch_label && (
                          <span className="rounded border border-kiviBorder bg-kiviElevated px-1.5 py-0.5 font-mono text-[9px] text-kiviSecondary">
                            {d.arch_label}
                          </span>
                        )}
                        {d.via_bridge && (
                          <span className="ml-1 rounded border border-kiviAccent/40 bg-kiviAccent/10 px-1.5 py-0.5 font-mono text-[9px] text-kiviAccent">
                            32-bit Bridge
                          </span>
                        )}
                      </span>
                      {d.loadable && (
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => onAttachJ2534(d.path)}
                          className="rounded border border-kiviAccent/40 px-2 py-0.5 text-[10px] text-kiviAccent hover:bg-kiviAccent/10"
                        >
                          Attach driver
                        </button>
                      )}
                    </div>
                    <span className="truncate font-mono text-[10px] text-kiviMuted">{d.path}</span>
                    <span className={`text-[10px] ${d.loadable ? 'text-kiviGreen' : 'text-kiviWarn'}`}>
                      {describeDriverUsability(d)}
                    </span>
                  </li>
                ))}
              </ul>
            )
          }
        />
      </div>

      {/* Collapsible Manual Cable / Bluetooth / Wi-Fi / Protocol Controls */}
      <div>
        <button
          type="button"
          aria-expanded={manualOpen}
          onClick={() => setManualOpen((v) => !v)}
          className="flex items-center gap-1 text-[11px] font-medium text-kiviSecondary hover:text-kiviInk"
        >
          {manualOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          <span>Manual Cable, ELM327 USB/BT/Wi-Fi, K-Line &amp; Bus Protocol</span>
        </button>

        {manualOpen && (
          <div className="mt-2 space-y-2.5 rounded border border-kiviBorder bg-kiviSurface p-2.5">
            {serialPorts.length > 0 && (
              <label className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-kiviMuted">
                <span>Detected ports</span>
                <select
                  aria-label="Detected serial ports"
                  value={serialPort}
                  onChange={(e) => onChangeSerialPort(e.target.value)}
                  className="w-64 rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-kiviInk"
                >
                  {serialPorts.map((p) => (
                    <option key={p.port_name} value={p.port_name}>
                      {p.port_name} ({portBadgeLabel(p)})
                    </option>
                  ))}
                </select>
              </label>
            )}
            <label className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-kiviMuted">
              <span>Serial / OpenPort / BT SPP port</span>
              <input
                type="text"
                aria-label="Serial port path"
                value={serialPort}
                onChange={(e) => onChangeSerialPort(e.target.value)}
                className="w-64 rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-kiviInk"
              />
            </label>

            <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
              <label className="space-y-1 text-[10px] text-kiviMuted">
                <span>ELM327 Bus Profile (Cars / Bikes)</span>
                <select
                  aria-label="ELM327 vehicle bus profile"
                  value={elmBusProfile}
                  onChange={(e) => onChangeElmBusProfile(e.target.value as ElmBusProfile)}
                  className="w-full rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[11px] text-kiviInk"
                >
                  {ELM_BUS_PROFILE_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="space-y-1 text-[10px] text-kiviMuted">
                <span>ELM327 Adapter Baud Rate</span>
                <select
                  aria-label="ELM327 serial baud rate"
                  value={elmBaudRate === undefined ? 'auto' : String(elmBaudRate)}
                  onChange={(e) =>
                    onChangeElmBaudRate(
                      e.target.value === 'auto' ? undefined : Number(e.target.value)
                    )
                  }
                  className="w-full rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[11px] text-kiviInk"
                >
                  <option value="auto">Auto-Probe (38400 / 115200 / 9600)</option>
                  <option value="38400">38400 bps (Cheap Clones / Bluetooth SPP)</option>
                  <option value="115200">115200 bps (STN / OBDLink / USB Fast)</option>
                  <option value="9600">9600 bps (Legacy ELM)</option>
                  <option value="230400">230400 bps</option>
                  <option value="500000">500000 bps</option>
                </select>
              </label>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                disabled={isBusy}
                onClick={() => onAttachSerial()}
                className="rounded border border-kiviAccent/40 bg-kiviAccent/10 px-2.5 py-1 text-[10px] font-medium text-kiviAccent hover:bg-kiviAccent/20"
              >
                Attach ELM327 USB / BT (Diag + Telemetry)
              </button>
              <button
                type="button"
                disabled={isBusy}
                onClick={() => onAttachOpenPort2()}
                className="rounded border border-kiviBorderStrong px-2 py-1 text-[10px] text-kiviSecondary hover:bg-kiviElevated"
              >
                Attach OpenPort 2.0
              </button>
              <select
                aria-label="Raw K-Line cable baud rate"
                value={klineBaudRate}
                onChange={(e) => onChangeKlineBaudRate(Number(e.target.value))}
                className="rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[10px] text-kiviInk"
              >
                <option value={10400}>10400 bps (Honda / ISO K-Line)</option>
                <option value={15625}>15625 bps (Yamaha 3-pin YDT)</option>
                <option value={62500}>62500 bps (Honda Fast)</option>
              </select>
              <button
                type="button"
                disabled={isBusy}
                onClick={() => onAttachKLineSerial()}
                className="rounded border border-kiviBorderStrong px-2 py-1 text-[10px] text-kiviSecondary hover:bg-kiviElevated"
              >
                Attach Raw K-Line USB
              </button>
            </div>

            <div className="flex flex-wrap items-center gap-1.5 pt-1">
              <input
                type="text"
                aria-label="ELM327 Wi-Fi host and port"
                placeholder="192.168.0.10:35000"
                value={elmWifiEndpoint}
                onChange={(e) => onChangeElmWifiEndpoint(e.target.value)}
                className="flex-1 rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[11px] text-kiviInk"
              />
              <button
                type="button"
                disabled={isBusy}
                onClick={onAttachElmWifi}
                className="rounded border border-kiviAccent/40 px-2.5 py-1 text-[10px] text-kiviAccent hover:bg-kiviAccent/10"
              >
                Attach ELM327 Wi-Fi
              </button>
            </div>

            <div className="flex items-center gap-1.5 pt-1">
              <input
                type="text"
                aria-label="Custom J2534 library path"
                placeholder="/path/to/j2534.dll | .so | .dylib"
                value={customLibPath}
                onChange={(e) => setCustomLibPath(e.target.value)}
                className="flex-1 rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[11px] text-kiviInk"
              />
              <button
                type="button"
                disabled={isBusy || !customLibPath.trim()}
                onClick={() => onAttachJ2534(customLibPath.trim())}
                className="rounded border border-kiviBorderStrong px-2 py-1 text-[10px] text-kiviSecondary hover:bg-kiviElevated disabled:cursor-not-allowed disabled:opacity-50"
              >
                Attach Library
              </button>
            </div>

            <div className="grid grid-cols-1 gap-2 pt-1 sm:grid-cols-2">
              <div className="flex items-center gap-1.5">
                <input
                  type="text"
                  aria-label="DoIP gateway host and port"
                  value={doipHost}
                  onChange={(e) => onChangeDoipHost(e.target.value)}
                  className="flex-1 rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[11px] text-kiviInk"
                />
                <button
                  type="button"
                  disabled={isBusy}
                  onClick={onAttachDoIp}
                  className="rounded border border-kiviBorderStrong px-2 py-1 text-[10px] text-kiviSecondary hover:bg-kiviElevated"
                >
                  Attach DoIP
                </button>
              </div>
              <div className="flex items-center gap-1.5">
                <input
                  type="text"
                  aria-label="SocketCAN interface"
                  value={canIface}
                  onChange={(e) => onChangeCanIface(e.target.value)}
                  className="flex-1 rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[11px] text-kiviInk"
                />
                <button
                  type="button"
                  disabled={isBusy}
                  onClick={onAttachSocketCan}
                  className="rounded border border-kiviBorderStrong px-2 py-1 text-[10px] text-kiviSecondary hover:bg-kiviElevated"
                >
                  Attach SocketCAN
                </button>
              </div>
            </div>

            {transport && (
              <div className="space-y-1.5 rounded border border-kiviBorder bg-kiviBg/50 p-2">
                <p className="text-[10px] font-semibold uppercase tracking-wide text-kiviMuted">
                  Active Channel Protocol &amp; Arbitration ID Override (OpenPort / J2534 / ELM / CAN)
                </p>
                <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
                  <select
                    aria-label="Override channel protocol"
                    value={reconfProto}
                    onChange={(e) => setReconfProto(e.target.value)}
                    className="rounded border border-kiviBorder bg-kiviSurface px-1.5 py-1 font-mono text-[10px] text-kiviInk"
                  >
                    <option value="iso15765">ISO 15765-4 CAN</option>
                    <option value="can">Raw CAN</option>
                    <option value="iso14230">KWP2000 (ISO 14230)</option>
                    <option value="iso9141">ISO 9141-2 K-Line</option>
                    <option value="j1850_vpw">J1850 VPW</option>
                    <option value="j1850_pwm">J1850 PWM</option>
                  </select>
                  <input
                    type="text"
                    aria-label="Override bus baud rate"
                    placeholder="500000"
                    value={reconfBaud}
                    onChange={(e) => setReconfBaud(e.target.value)}
                    className="rounded border border-kiviBorder bg-kiviSurface px-1.5 py-1 font-mono text-[10px] text-kiviInk"
                  />
                  <input
                    type="text"
                    aria-label="Override TX CAN ID"
                    placeholder="TX 0x7E0"
                    value={reconfTxId}
                    onChange={(e) => setReconfTxId(e.target.value)}
                    className="rounded border border-kiviBorder bg-kiviSurface px-1.5 py-1 font-mono text-[10px] text-kiviInk"
                  />
                  <input
                    type="text"
                    aria-label="Override RX CAN ID"
                    placeholder="RX 0x7E8"
                    value={reconfRxId}
                    onChange={(e) => setReconfRxId(e.target.value)}
                    className="rounded border border-kiviBorder bg-kiviSurface px-1.5 py-1 font-mono text-[10px] text-kiviInk"
                  />
                </div>
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <label className="flex items-center gap-1.5 text-[10px] text-kiviSecondary">
                    <input
                      type="checkbox"
                      checked={reconfKlineNoCs}
                      onChange={(e) => setReconfKlineNoCs(e.target.checked)}
                    />
                    <span>ISO9141_NO_CHECKSUM (Honda K-Line)</span>
                  </label>
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={() =>
                      onReconfigureTransport(
                        reconfProto,
                        Number(reconfBaud) || 500000,
                        reconfTxId,
                        reconfRxId,
                        reconfKlineNoCs
                      )
                    }
                    className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-[10px] text-kiviAccent hover:bg-kiviAccent/25"
                  >
                    Apply Protocol / IDs
                  </button>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
