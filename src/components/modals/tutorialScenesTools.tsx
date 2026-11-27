import { MONO, PinBadge, focusStroke } from './tutorialCutoutPrimitives';

export const renderHexReverseScene = (activePin: number | null, onSelectPin?: (p: number) => void) => (
  <>
    <g>
      <rect x={22} y={20} width={516} height={32} rx={7} fill="var(--kivi-elevated)" {...focusStroke(activePin, 1)} />
      {[
        { label: '8b / 16b', x: 30, w: 66, tone: 'neutral' },
        { label: 'Hex / Dec', x: 102, w: 68, tone: 'neutral' },
        { label: '- 16B +', x: 176, w: 58, tone: 'neutral' },
        { label: 'Diff', x: 240, w: 46, tone: 'ok' },
        { label: '< Maps (13) >', x: 292, w: 98, tone: 'warn' },
        { label: 'Promote Map', x: 396, w: 132, tone: 'accent' },
      ].map((ctrl) => (
        <g key={ctrl.label}>
          <rect
            x={ctrl.x}
            y={25}
            width={ctrl.w}
            height={22}
            rx={4}
            fill={ctrl.tone === 'neutral' ? 'var(--kivi-surface)' : `var(--kivi-${ctrl.tone})`}
            fillOpacity={ctrl.tone === 'neutral' ? 1 : 0.14}
            stroke={ctrl.tone === 'neutral' ? 'var(--kivi-border)' : `var(--kivi-${ctrl.tone})`}
            strokeWidth={0.9}
          />
          <text x={ctrl.x + ctrl.w / 2} y={39} textAnchor="middle" fontFamily={MONO} fontSize={8} fontWeight={700} fill="var(--kivi-ink)">
            {ctrl.label}
          </text>
        </g>
      ))}
    </g>

    <g>
      <rect x={22} y={62} width={332} height={186} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 2)} />
      <text x={34} y={82} fontFamily={MONO} fontSize={8.4} fill="var(--kivi-muted)">00000000</text>
      <rect x={92} y={71} width={22} height={16} rx={3} fill="var(--kivi-ok)" fillOpacity={0.18} stroke="var(--kivi-accent)" />
      <text x={103} y={82} textAnchor="middle" fontFamily={MONO} fontSize={8.4} fontWeight={700} fill="var(--kivi-ink)">81</text>
      <text x={122} y={82} fontFamily={MONO} fontSize={8.4} fill="var(--kivi-secondary)">
        00  1A  2C  4F  00  FF  08
      </text>

      <rect x={30} y={96} width={316} height={54} rx={5} fill="var(--kivi-warn)" fillOpacity={0.1} stroke="var(--kivi-warn)" strokeOpacity={0.55} />
      <text x={36} y={113} fontFamily={MONO} fontSize={8.2} fontWeight={600} fill="var(--kivi-ink)">
        00000060  48 52 5C 66 70 7A 84 8E  [Potential Map]
      </text>
      <text x={36} y={132} fontFamily={MONO} fontSize={8.2} fill="var(--kivi-secondary)">
        00000070  4C 56 60 6B 75 7F 8A 94
      </text>

      <rect x={30} y={196} width={316} height={42} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={38} y={212} fontFamily={MONO} fontSize={8} fontWeight={700} fill="var(--kivi-accent)">
        @ 0x000000 BE  u8: 129   i8: -127
      </text>
      <text x={38} y={228} fontFamily={MONO} fontSize={8} fill="var(--kivi-secondary)">
        u16: 33024   i16: -32512   f32: 0.000
      </text>
    </g>

    <g>
      <rect x={366} y={62} width={172} height={186} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 3)} />
      <text x={378} y={82} fontFamily={MONO} fontSize={8.5} fontWeight={700} fill="var(--kivi-ink)">CHANGE LEDGER</text>
      <text x={526} y={82} textAnchor="end" fontFamily={MONO} fontSize={8.2} fontWeight={700} fill="var(--kivi-warn)">1 B</text>
      <rect x={376} y={92} width={152} height={42} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-warn)" />
      <text x={384} y={108} fontFamily={MONO} fontSize={8.2} fontWeight={700} fill="var(--kivi-ink)">0x0000   1B</text>
      <text x={384} y={123} fontFamily={MONO} fontSize={8} fill="var(--kivi-warn)">00 -&gt; 81  (Jump Hex)</text>
      <text x={378} y={160} fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">Klik baris klaster untuk</text>
      <text x={378} y={173} fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">melompat ke offset Hex.</text>
    </g>

    <PinBadge x={532} y={22} pin={1} active={activePin === 1} onSelect={onSelectPin} />
    <PinBadge x={340} y={114} pin={2} active={activePin === 2} onSelect={onSelectPin} />
    <PinBadge x={528} y={60} pin={3} active={activePin === 3} onSelect={onSelectPin} />
  </>
);

export const renderDiffChecksumScene = (activePin: number | null, onSelectPin?: (p: number) => void) => (
  <>
    <g>
      <rect x={22} y={20} width={264} height={228} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 1)} />
      <text x={36} y={40} fontFamily={MONO} fontSize={9} fontWeight={700} fill="var(--kivi-ink)">
        BINARY DIFF ENGINE (Ctrl+B)
      </text>
      {[
        { addr: '0x01400', kind: 'Table2D', delta: '4C -> 52', top: 54 },
        { addr: '0x02810', kind: 'Scalar', delta: '00 -> 81', top: 108 },
      ].map((row) => (
        <g key={row.addr}>
          <rect x={34} y={row.top} width={240} height={44} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
          <text x={44} y={row.top + 17} fontFamily={MONO} fontSize={8.5} fontWeight={700} fill="var(--kivi-accent)">{row.addr}</text>
          <text x={110} y={row.top + 17} fontFamily={MONO} fontSize={8.2} fill="var(--kivi-ink)">{row.kind}</text>
          <text x={44} y={row.top + 34} fontFamily={MONO} fontSize={8} fill="var(--kivi-warn)">Delta: {row.delta}</text>
          <rect x={208} y={row.top + 12} width={58} height={20} rx={4} fill="var(--kivi-accent)" fillOpacity={0.14} stroke="var(--kivi-accent)" />
          <text x={237} y={row.top + 25} textAnchor="middle" fontFamily={MONO} fontSize={7.8} fontWeight={700} fill="var(--kivi-ink)">Extract</text>
        </g>
      ))}
      <text x={36} y={226} fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">Ekstrak blok diff langsung ke peta baru</text>
    </g>

    <g>
      <rect x={298} y={20} width={240} height={116} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 2)} />
      <text x={312} y={40} fontFamily={MONO} fontSize={9} fontWeight={700} fill="var(--kivi-ink)">CHECKSUM &amp; INTEGRITY</text>
      <rect x={312} y={50} width={212} height={28} rx={6} fill="var(--kivi-ok)" fillOpacity={0.14} stroke="var(--kivi-ok)" />
      <text x={418} y={68} textAnchor="middle" fontFamily={MONO} fontSize={8.6} fontWeight={700} fill="var(--kivi-ink)">VALID (Keihin / OEM Family)</text>
      <rect x={312} y={88} width={102} height={32} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border-strong)" />
      <text x={363} y={108} textAnchor="middle" fontFamily={MONO} fontSize={8} fontWeight={600} fill="var(--kivi-ink)">Verify Checksum</text>
      <rect x={422} y={88} width={102} height={32} rx={6} fill="var(--kivi-accent)" />
      <text x={473} y={108} textAnchor="middle" fontFamily={MONO} fontSize={8} fontWeight={700} fill="var(--kivi-accent-fg)">Auto-Fix</text>
    </g>

    <g>
      <rect x={298} y={148} width={240} height={100} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 3)} />
      <text x={312} y={168} fontFamily={MONO} fontSize={9} fontWeight={700} fill="var(--kivi-ink)">FILE -&gt; SAVE AS...</text>
      <rect x={312} y={178} width={100} height={30} rx={5} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={362} y={197} textAnchor="middle" fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">stock.bin</text>
      <text x={418} y={197} textAnchor="middle" fontFamily={MONO} fontSize={10} fill="var(--kivi-accent)">-&gt;</text>
      <rect x={426} y={178} width={98} height={30} rx={5} fill="var(--kivi-elevated)" stroke="var(--kivi-accent)" />
      <text x={475} y={197} textAnchor="middle" fontFamily={MONO} fontSize={8} fontWeight={700} fill="var(--kivi-ink)">tuned.bin</text>
      <text x={312} y={230} fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">Ctrl+Shift+S menjaga file stok tetap aman</text>
    </g>

    <PinBadge x={274} y={36} pin={1} active={activePin === 1} onSelect={onSelectPin} />
    <PinBadge x={526} y={36} pin={2} active={activePin === 2} onSelect={onSelectPin} />
    <PinBadge x={526} y={164} pin={3} active={activePin === 3} onSelect={onSelectPin} />
  </>
);

export const renderTelemetryEcuScene = (activePin: number | null, onSelectPin?: (p: number) => void) => (
  <>
    <g>
      <rect x={22} y={20} width={248} height={228} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 1)} />
      <text x={36} y={40} fontFamily={MONO} fontSize={8.8} fontWeight={700} fill="var(--kivi-ink)">TELEMETRY COCKPIT</text>
      {Array.from({ length: 12 }).map((_, i) => {
        const fill = i < 6 ? 'var(--kivi-ok)' : i < 10 ? 'var(--kivi-warn)' : 'var(--kivi-danger)';
        return <rect key={i} x={36 + i * 18} y={50} width={14} height={7} rx={2} fill={fill} />;
      })}
      <rect x={36} y={68} width={220} height={82} rx={8} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={146} y={114} textAnchor="middle" fontFamily={MONO} fontSize={30} fontWeight={700} fill="var(--kivi-ink)">8,650</text>
      <text x={146} y={136} textAnchor="middle" fontFamily={MONO} fontSize={8.5} fontWeight={700} fill="var(--kivi-accent)">
        RPM  |  GEAR 3  |  ARC / TRACK
      </text>
      {[
        { label: 'KM/H', val: '112', x: 36 },
        { label: 'AFR', val: '12.8', x: 112 },
        { label: 'ECT', val: '88C', x: 188 },
      ].map((pod) => (
        <g key={pod.label}>
          <rect x={pod.x} y={162} width={68} height={46} rx={6} fill="var(--kivi-elevated)" stroke="var(--kivi-border)" />
          <text x={pod.x + 34} y={178} textAnchor="middle" fontFamily={MONO} fontSize={7.8} fill="var(--kivi-muted)">{pod.label}</text>
          <text x={pod.x + 34} y={197} textAnchor="middle" fontFamily={MONO} fontSize={10.5} fontWeight={700} fill="var(--kivi-ink)">{pod.val}</text>
        </g>
      ))}
      <text x={36} y={230} fontFamily={MONO} fontSize={7.8} fill="var(--kivi-muted)">Layout terkunci stabil 0.0px saat sweep</text>
    </g>

    <g>
      <rect x={282} y={20} width={256} height={118} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 2)} />
      <rect x={294} y={32} width={104} height={22} rx={4} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={346} y={46} textAnchor="middle" fontFamily={MONO} fontSize={7.8} fill="var(--kivi-muted)">Read &amp; Flash</text>
      <rect x={404} y={32} width={122} height={22} rx={4} fill="var(--kivi-accent)" fillOpacity={0.14} stroke="var(--kivi-accent)" />
      <text x={465} y={46} textAnchor="middle" fontFamily={MONO} fontSize={7.8} fontWeight={700} fill="var(--kivi-ink)">ELM327 / OBD</text>
      {[
        { label: 'Read DTCs', x: 294, tone: 'accent' },
        { label: 'Clear DTCs', x: 374, tone: 'warn' },
        { label: 'Reset ECU', x: 454, tone: 'danger' },
      ].map((act) => (
        <g key={act.label}>
          <rect x={act.x} y={68} width={74} height={28} rx={5} fill={`var(--kivi-${act.tone})`} fillOpacity={0.14} stroke={`var(--kivi-${act.tone})`} />
          <text x={act.x + 37} y={85} textAnchor="middle" fontFamily={MONO} fontSize={7.8} fontWeight={700} fill="var(--kivi-ink)">{act.label}</text>
        </g>
      ))}
      <text x={294} y={118} fontFamily={MONO} fontSize={8} fill="var(--kivi-secondary)">Baca, hapus DTC, &amp; reset ECU terkonfirmasi</text>
    </g>

    <g>
      <rect x={282} y={150} width={256} height={98} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 3)} />
      <circle cx={298} cy={167} r={3.5} fill="var(--kivi-danger)" />
      <circle cx={309} cy={167} r={3.5} fill="var(--kivi-warn)" />
      <circle cx={320} cy={167} r={3.5} fill="var(--kivi-ok)" />
      <text x={334} y={170} fontFamily={MONO} fontSize={8.2} fontWeight={700} fill="var(--kivi-ink)">Multi-Window &amp; Flash ROM</text>
      <rect x={294} y={182} width={232} height={24} rx={5} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={304} y={197} fontFamily={MONO} fontSize={8} fill="var(--kivi-secondary)">Preset: Honda CBR250RR (MC51)</text>
      <text x={294} y={228} fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">Geser bebas, Pin ke atas, Resize 8 sisi</text>
    </g>

    <PinBadge x={258} y={36} pin={1} active={activePin === 1} onSelect={onSelectPin} />
    <PinBadge x={530} y={24} pin={2} active={activePin === 2} onSelect={onSelectPin} />
    <PinBadge x={526} y={166} pin={3} active={activePin === 3} onSelect={onSelectPin} />
  </>
);

export const renderDiagMotoUdsScene = (activePin: number | null, onSelectPin?: (p: number) => void) => (
  <>
    <g>
      <rect x={20} y={16} width={520} height={30} rx={7} fill="var(--kivi-elevated)" {...focusStroke(activePin, 1)} />
      {[
        { label: 'Overview', x: 26, w: 92, active: true },
        { label: 'Live PIDs', x: 124, w: 92, active: false },
        { label: 'UDS 0x31/2F', x: 222, w: 98, active: false },
        { label: 'Moto Lab', x: 326, w: 96, active: false },
        { label: 'Terminal', x: 428, w: 104, active: false },
      ].map((tab) => (
        <g key={tab.label}>
          <rect
            x={tab.x}
            y={21}
            width={tab.w}
            height={20}
            rx={4}
            fill={tab.active ? 'var(--kivi-accent)' : 'var(--kivi-surface)'}
            fillOpacity={tab.active ? 0.18 : 1}
            stroke={tab.active ? 'var(--kivi-accent)' : 'var(--kivi-border)'}
          />
          <text
            x={tab.x + tab.w / 2}
            y={34}
            textAnchor="middle"
            fontFamily={MONO}
            fontSize={7.8}
            fontWeight={700}
            fill={tab.active ? 'var(--kivi-accent)' : 'var(--kivi-secondary)'}
          >
            {tab.label}
          </text>
        </g>
      ))}
    </g>

    <g>
      <rect x={20} y={54} width={250} height={96} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 1)} />
      <text x={32} y={72} fontFamily={MONO} fontSize={8.2} fontWeight={700} fill="var(--kivi-ink)">
        Multi-ECU &amp; 54+ OBD PIDs
      </text>
      <text x={32} y={90} fontFamily={MONO} fontSize={8} fill="var(--kivi-ok)">
        0x7E0 ECM | 0x7E1 TCM | 29-bit
      </text>
      <text x={32} y={108} fontFamily={MONO} fontSize={8} fill="var(--kivi-warn)">
        P0171 Lean B1 + Freeze Frame
      </text>
      <text x={32} y={126} fontFamily={MONO} fontSize={7.8} fill="var(--kivi-secondary)">
        I/M Readiness + Mode 06 / 0A
      </text>
    </g>

    <g>
      <rect x={20} y={158} width={250} height={92} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 2)} />
      <text x={32} y={176} fontFamily={MONO} fontSize={8.2} fontWeight={700} fill="var(--kivi-ink)">
        UDS Service &amp; Active Tests
      </text>
      <rect x={32} y={186} width={104} height={20} rx={4} fill="var(--kivi-warn)" fillOpacity={0.14} stroke="var(--kivi-warn)" />
      <text x={84} y={199} textAnchor="middle" fontFamily={MONO} fontSize={7.8} fontWeight={700} fill="var(--kivi-warn)">
        0x31 Routine Reset
      </text>
      <rect x={144} y={186} width={114} height={20} rx={4} fill="var(--kivi-ok)" fillOpacity={0.14} stroke="var(--kivi-ok)" />
      <text x={201} y={199} textAnchor="middle" fontFamily={MONO} fontSize={7.6} fontWeight={700} fill="var(--kivi-ok)">
        Return Control 0x00
      </text>
      <text x={32} y={228} fontFamily={MONO} fontSize={7.8} fill="var(--kivi-muted)">
        Session 0x10 | DID 0x22 | IO 0x2F
      </text>
    </g>

    <g>
      <rect x={282} y={54} width={258} height={196} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 3)} />
      <text x={294} y={73} fontFamily={MONO} fontSize={8.4} fontWeight={700} fill="var(--kivi-ink)">
        Motorcycle Lab &amp; Raw Terminal
      </text>
      <rect x={294} y={84} width={234} height={48} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={304} y={100} fontFamily={MONO} fontSize={8} fill="var(--kivi-accent)">
        Honda 4-Pin (0x72) | Euro 5 CAN
      </text>
      <text x={304} y={118} fontFamily={MONO} fontSize={7.8} fill="var(--kivi-secondary)">
        Yamaha 3/4P | KDS | SDS (10.4k)
      </text>
      <rect x={294} y={140} width={234} height={50} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={304} y={156} fontFamily={MONO} fontSize={7.8} fill="var(--kivi-ok)">
        Pinout: P4/5 GND | P7 K-Line | P16 +12V
      </text>
      <text x={304} y={174} fontFamily={MONO} fontSize={7.6} fill="var(--kivi-muted)">
        KKL FTDI / OpenPort 2.0 / ELM v1.5
      </text>
      <rect x={294} y={198} width={234} height={40} rx={6} fill="var(--kivi-elevated)" stroke="var(--kivi-borderStrong)" />
      <text x={304} y={214} fontFamily={MONO} fontSize={8} fill="var(--kivi-ink)">
        &gt; ATI -&gt; ELM327 v1.5 [pic18]
      </text>
      <text x={304} y={229} fontFamily={MONO} fontSize={7.6} fill="var(--kivi-secondary)">
        Wi-Fi 192.168.0.10:35000 / BT SPP
      </text>
    </g>

    <PinBadge x={260} y={66} pin={1} active={activePin === 1} onSelect={onSelectPin} />
    <PinBadge x={260} y={170} pin={2} active={activePin === 2} onSelect={onSelectPin} />
    <PinBadge x={530} y={66} pin={3} active={activePin === 3} onSelect={onSelectPin} />
  </>
);
