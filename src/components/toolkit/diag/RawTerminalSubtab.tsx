import React, { useState } from 'react';
import { Cpu, Send, Terminal, Trash2 } from 'lucide-react';
import {
  runSendRawDiagCommand,
  type ElmCapabilityReport,
  type FlashState,
} from '../flashController';

export interface RawTerminalSubtabProps {
  isBusy: boolean;
  hasDevice: boolean;
  elmCapabilities: FlashState<ElmCapabilityReport> | null;
  onProbeElmCapabilities: () => void;
  onLog: (level: 'info' | 'ok' | 'warn' | 'error', message: string) => void;
}

interface TerminalLine {
  id: number;
  cmd: string;
  reply: string;
  isError: boolean;
}

const QUICK_CHIPS = ['ATI', 'ATRV', 'ATDPN', 'STDI', '01 00', '03', '09 02'];

export const RawTerminalSubtab: React.FC<RawTerminalSubtabProps> = ({
  isBusy,
  hasDevice,
  elmCapabilities,
  onProbeElmCapabilities,
  onLog,
}) => {
  const [commandInput, setCommandInput] = useState('ATI');
  const [sending, setSending] = useState(false);
  const [lines, setLines] = useState<TerminalLine[]>([]);

  const sendCommand = async (rawCmd?: string) => {
    const cmd = (rawCmd ?? commandInput).trim();
    if (!cmd) return;
    setSending(true);
    try {
      const res = await runSendRawDiagCommand(cmd);
      const isError = res.kind !== 'ok';
      const reply = res.kind === 'ok' ? res.data : res.message;
      setLines((prev) => [...prev.slice(-49), { id: Date.now() + prev.length, cmd, reply, isError }]);
      onLog(isError ? 'warn' : 'info', `RAW "${cmd}" -> ${reply}`);
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="space-y-3" data-testid="raw-terminal-subtab">
      <section className="space-y-2.5 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Terminal className="h-3.5 w-3.5 text-kiviAccent" />
            <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
              Interactive ELM327 / AT / OBD Raw Console
            </h3>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={isBusy || !hasDevice}
              onClick={onProbeElmCapabilities}
              className="inline-flex min-h-[30px] items-center gap-1 rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2 py-0.5 font-mono text-[10px] text-kiviAccent hover:bg-kiviAccent/25 disabled:opacity-50"
            >
              <Cpu className="h-3 w-3" />
              <span>Probe Clone AT</span>
            </button>
            <button
              type="button"
              onClick={() => setLines([])}
              className="inline-flex min-h-[30px] items-center gap-1 rounded border border-kiviBorder bg-kiviSurface px-2 py-0.5 font-mono text-[10px] text-kiviMuted hover:text-kiviInk"
            >
              <Trash2 className="h-3 w-3" />
              <span>Clear</span>
            </button>
          </div>
        </div>

        <div className="flex flex-wrap gap-1.5">
          {QUICK_CHIPS.map((chip) => (
            <button
              key={chip}
              type="button"
              disabled={isBusy || sending || !hasDevice}
              onClick={() => {
                setCommandInput(chip);
                void sendCommand(chip);
              }}
              className="rounded border border-kiviBorder bg-kiviSurface px-2 py-0.5 font-mono text-[10px] text-kiviSecondary hover:border-kiviAccent hover:text-kiviInk disabled:opacity-50"
            >
              {chip}
            </button>
          ))}
        </div>

        <form
          onSubmit={(e) => {
            e.preventDefault();
            void sendCommand();
          }}
          className="flex gap-2"
        >
          <input
            type="text"
            aria-label="Raw AT or OBD hex command"
            value={commandInput}
            onChange={(e) => setCommandInput(e.target.value)}
            placeholder="Type AT command or OBD hex (e.g. ATSH 7E0, 01 0C, 22 F1 90)"
            className="flex-1 rounded border border-kiviBorderStrong bg-kiviSurface px-2.5 py-1 font-mono text-[11px] text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
          />
          <button
            type="submit"
            disabled={isBusy || sending || !hasDevice}
            className="inline-flex min-h-[32px] items-center gap-1.5 rounded bg-kiviAccent/20 px-3 py-1 font-mono text-[11px] font-semibold text-kiviAccent hover:bg-kiviAccent/30 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:opacity-50"
          >
            <Send className="h-3 w-3" />
            <span>Send</span>
          </button>
        </form>

        {elmCapabilities?.kind === 'ok' && (
          <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[10px] text-kiviSecondary">
            <div>
              {elmCapabilities.data.adapter_banner} | Tier: {elmCapabilities.data.chip_tier} | Protocol:{' '}
              {elmCapabilities.data.active_protocol}
            </div>
            <div>
              ATSH: {String(elmCapabilities.data.supports_atsh)} | ATCAF0:{' '}
              {String(elmCapabilities.data.supports_atcaf0)} | ATTP5 K-Line:{' '}
              {String(elmCapabilities.data.supports_kline_atsp5)} | STDI:{' '}
              {String(elmCapabilities.data.supports_st_commands)}
            </div>
          </div>
        )}

        <div className="max-h-56 space-y-1 overflow-y-auto rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[11px]">
          {lines.length === 0 ? (
            <p className="text-kiviMuted">
              Connect an ELM327 USB, Bluetooth SPP, or Wi-Fi adapter and send any AT/OBD hex query.
            </p>
          ) : (
            lines.map((item) => (
              <div key={item.id} className="space-y-0.5 border-b border-kiviBorder/40 pb-1 last:border-b-0">
                <div className="text-kiviAccent">&gt; {item.cmd}</div>
                <pre
                  className={`whitespace-pre-wrap break-all ${
                    item.isError ? 'text-kiviRed' : 'text-kiviInk'
                  }`}
                >
                  {item.reply}
                </pre>
              </div>
            ))
          )}
        </div>
      </section>
    </div>
  );
};
