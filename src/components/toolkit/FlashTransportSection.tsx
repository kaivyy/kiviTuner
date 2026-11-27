import React, { useState } from 'react';
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Cpu,
  Loader2,
  RefreshCw,
  Usb,
  Wand2,
} from 'lucide-react';
import {
  describeDeviceRisk,
  describeDriverUsability,
  describeTransportCapability,
  type DiscoveredDriverView,
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
  doipHost: string;
  onChangeDoipHost: (val: string) => void;
  canIface: string;
  onChangeCanIface: (val: string) => void;
  onAutoConnect: () => void;
  onRefresh: () => void;
  onDetach: () => void;
  onAttachOpenPort2: (port?: string) => void;
  onAttachSerial: (port?: string) => void;
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
  if (p.adapter_hint === 'tactrix_clone_candidate') return 'Tactrix / USB-CDC candidate';
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
  doipHost,
  onChangeDoipHost,
  canIface,
  onChangeCanIface,
  onAutoConnect,
  onRefresh,
  onDetach,
  onAttachOpenPort2,
  onAttachSerial,
  onAttachJ2534,
  onAttachDoIp,
  onAttachSocketCan,
}) => {
  const [manualOpen, setManualOpen] = useState(false);
  const [customLibPath, setCustomLibPath] = useState('');

  const risk = activeDevice ? describeDeviceRisk(activeDevice) : null;
  const openPortCandidates = serialPorts.filter(
    (p) => p.is_openport2 || p.adapter_hint === 'tactrix_clone_candidate'
  );
  const driverList = drivers && drivers.kind === 'ok' ? drivers.data : [];
  const has32BitDllWarning = driverList.some(
    (d) => !d.loadable && (d.arch_label?.includes('32-bit') || d.reason?.includes('32-bit'))
  );

  return (
    <div className="space-y-3 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
      {/* Header + Auto-Detect Primary Action -------------------------- */}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Usb className="h-3.5 w-3.5 text-kiviAccent" />
          <h3 className="text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
            Hardware Cable &amp; J2534 Drivers
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
        <p className="text-[11px] text-kiviSecondary">{describeTransportCapability(transport)}</p>
        {transportError && <p className="text-[11px] text-kiviWarn">{transportError}</p>}
        <StateLine
          state={deviceState}
          render={(list) =>
            list.length === 0 ? (
              <span className="text-kiviMuted">
                No flash device attached. Plug USB cable (OpenPort 2.0 / clone / J2534) then click Auto-Detect.
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
            Detected USB-CDC OpenPort / Clone Ports (No DLL Required)
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
                <button
                  type="button"
                  disabled={isBusy}
                  onClick={() => onAttachOpenPort2(p.port_name)}
                  className="rounded border border-kiviAccent/40 px-2 py-0.5 text-[10px] text-kiviAccent hover:bg-kiviAccent/10"
                >
                  Attach OpenPort 2.0
                </button>
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
            If using Tactrix OpenPort 2.0 (original or clone), use native OpenPort 2.0 USB-CDC above instead of op20pt32.dll.
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

      {/* Collapsible Manual Cable / Library Controls ------------------- */}
      <div>
        <button
          type="button"
          aria-expanded={manualOpen}
          onClick={() => setManualOpen((v) => !v)}
          className="flex items-center gap-1 text-[11px] font-medium text-kiviSecondary hover:text-kiviInk"
        >
          {manualOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          <span>Advanced: Manual Cable / Library Path</span>
        </button>

        {manualOpen && (
          <div className="mt-2 space-y-2 rounded border border-kiviBorder bg-kiviSurface p-2.5">
            {serialPorts.length > 0 && (
              <label className="flex items-center justify-between gap-2 text-[11px] text-kiviMuted">
                <span>Detected ports</span>
                <select
                  aria-label="Detected serial ports"
                  value={serialPort}
                  onChange={(e) => onChangeSerialPort(e.target.value)}
                  className="w-56 rounded border border-kiviBorder bg-kiviBg px-1.5 py-0.5 font-mono text-kiviInk"
                >
                  {serialPorts.map((p) => (
                    <option key={p.port_name} value={p.port_name}>
                      {p.port_name} ({portBadgeLabel(p)})
                    </option>
                  ))}
                </select>
              </label>
            )}
            <label className="flex items-center justify-between gap-2 text-[11px] text-kiviMuted">
              <span>Serial / OpenPort 2.0 port</span>
              <input
                type="text"
                aria-label="Serial port path"
                value={serialPort}
                onChange={(e) => onChangeSerialPort(e.target.value)}
                className="w-48 rounded border border-kiviBorder bg-kiviBg px-1.5 py-0.5 font-mono text-kiviInk"
              />
            </label>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                disabled={isBusy}
                onClick={() => onAttachOpenPort2()}
                className="rounded border border-kiviAccent/40 px-2 py-1 text-[10px] text-kiviAccent hover:bg-kiviAccent/10"
              >
                Attach OpenPort 2.0
              </button>
              <button
                type="button"
                disabled={isBusy}
                onClick={() => onAttachSerial()}
                className="rounded border border-kiviBorderStrong px-2 py-1 text-[10px] text-kiviSecondary hover:bg-kiviElevated"
              >
                Attach ELM327 serial (read-only)
              </button>
            </div>
            <div className="flex items-center gap-1.5 pt-1">
              <input
                type="text"
                aria-label="Custom J2534 library path"
                placeholder="/path/to/j2534.dll | .so | .dylib"
                value={customLibPath}
                onChange={(e) => setCustomLibPath(e.target.value)}
                className="flex-1 rounded border border-kiviBorder bg-kiviBg px-1.5 py-0.5 font-mono text-[11px] text-kiviInk"
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
                  className="flex-1 rounded border border-kiviBorder bg-kiviBg px-1.5 py-0.5 font-mono text-[11px] text-kiviInk"
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
                  className="flex-1 rounded border border-kiviBorder bg-kiviBg px-1.5 py-0.5 font-mono text-[11px] text-kiviInk"
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
          </div>
        )}
      </div>
    </div>
  );
};
