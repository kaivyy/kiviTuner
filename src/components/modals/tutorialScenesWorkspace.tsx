import { MONO, PinBadge, focusStroke } from './tutorialCutoutPrimitives';

export const renderWorkspaceScene = (activePin: number | null, onSelectPin?: (p: number) => void) => (
  <>
    <g>
      <rect x={22} y={20} width={516} height={38} rx={8} fill="var(--kivi-elevated)" {...focusStroke(activePin, 1)} />
      <circle cx={38} cy={39} r={4} fill="var(--kivi-danger)" />
      <circle cx={51} cy={39} r={4} fill="var(--kivi-warn)" />
      <circle cx={64} cy={39} r={4} fill="var(--kivi-ok)" />
      <text x={82} y={42} fontFamily={MONO} fontSize={9.5} fontWeight={700} fill="var(--kivi-ink)">
        kiviTuner
      </text>
      <text x={150} y={42} fontFamily={MONO} fontSize={8.8} fill="var(--kivi-secondary)">
        File   Edit   XDF   View   Reverse   ECU   Help
      </text>
      <rect x={410} y={28} width={60} height={22} rx={5} fill="var(--kivi-ok)" fillOpacity={0.14} stroke="var(--kivi-ok)" strokeWidth={0.9} />
      <text x={440} y={42} textAnchor="middle" fontFamily={MONO} fontSize={8.5} fontWeight={700} fill="var(--kivi-ink)">
        CHK OK
      </text>
      <rect x={476} y={28} width={52} height={22} rx={5} fill="var(--kivi-warn)" fillOpacity={0.14} stroke="var(--kivi-warn)" strokeWidth={0.9} />
      <text x={502} y={42} textAnchor="middle" fontFamily={MONO} fontSize={8.5} fontWeight={700} fill="var(--kivi-ink)">
        1B Diff
      </text>
    </g>

    <g>
      <rect x={22} y={70} width={58} height={178} rx={8} fill="var(--kivi-elevated)" {...focusStroke(activePin, 2)} />
      {[
        { label: 'Maps', active: true, y: 80 },
        { label: 'Insp', active: false, y: 120 },
        { label: 'Hex', active: false, y: 160 },
        { label: 'Theme', active: false, y: 200 },
      ].map((item) => (
        <g key={item.label}>
          <rect
            x={30}
            y={item.y}
            width={42}
            height={32}
            rx={6}
            fill={item.active ? 'var(--kivi-accent)' : 'var(--kivi-surface)'}
            fillOpacity={item.active ? 0.16 : 1}
            stroke={item.active ? 'var(--kivi-accent)' : 'var(--kivi-border)'}
            strokeWidth={0.9}
          />
          <text
            x={51}
            y={item.y + 19}
            textAnchor="middle"
            fontFamily={MONO}
            fontSize={8.2}
            fontWeight={item.active ? 700 : 500}
            fill={item.active ? 'var(--kivi-ink)' : 'var(--kivi-secondary)'}
          >
            {item.label}
          </text>
        </g>
      ))}
    </g>

    <g>
      <rect x={92} y={70} width={446} height={178} rx={8} fill="var(--kivi-surface)" {...focusStroke(activePin, 3)} />
      <rect x={102} y={78} width={118} height={24} rx={5} fill="var(--kivi-elevated)" stroke="var(--kivi-accent)" />
      <text x={112} y={93} fontFamily={MONO} fontSize={8.8} fontWeight={600} fill="var(--kivi-ink)">
        Main Fuel 16x16
      </text>
      <rect x={226} y={78} width={76} height={24} rx={5} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={264} y={93} textAnchor="middle" fontFamily={MONO} fontSize={8.5} fill="var(--kivi-muted)">
        Hexdump
      </text>
      <rect x={308} y={78} width={88} height={24} rx={5} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={352} y={93} textAnchor="middle" fontFamily={MONO} fontSize={8.5} fill="var(--kivi-muted)">
        Split Pane
      </text>
      <rect x={442} y={78} width={84} height={24} rx={5} fill="var(--kivi-elevated)" stroke="var(--kivi-border-strong)" />
      <text x={484} y={93} textAnchor="middle" fontFamily={MONO} fontSize={8.2} fontWeight={700} fill="var(--kivi-ink)">
        Ctrl+K Menu
      </text>

      <rect x={102} y={112} width={212} height={124} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={114} y={129} fontFamily={MONO} fontSize={8.5} fontWeight={600} fill="var(--kivi-secondary)">
        PANE A: 2D TABLE MATRIX
      </text>
      {[0, 1, 2, 3].map((r) =>
        [0, 1, 2, 3].map((c) => (
          <rect
            key={`${r}-${c}`}
            x={114 + c * 46}
            y={138 + r * 21}
            width={42}
            height={17}
            rx={3}
            fill={c >= 2 && r >= 1 ? 'var(--kivi-accent)' : 'var(--kivi-elevated)'}
            fillOpacity={c >= 2 && r >= 1 ? 0.22 : 1}
            stroke="var(--kivi-border)"
            strokeWidth={0.7}
          />
        ))
      )}

      <rect x={324} y={112} width={202} height={124} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border)" strokeDasharray="4 3" />
      <text x={336} y={129} fontFamily={MONO} fontSize={8.5} fontWeight={600} fill="var(--kivi-secondary)">
        PANE B: SPLIT PREVIEW
      </text>
      <path d="M 340 210 Q 385 162 426 178 T 512 146" fill="none" stroke="var(--kivi-accent)" strokeWidth={2} />
      <path d="M 340 218 L 512 172" fill="none" stroke="var(--kivi-muted)" strokeWidth={1.1} strokeDasharray="3 3" />
    </g>

    <PinBadge x={526} y={24} pin={1} active={activePin === 1} onSelect={onSelectPin} />
    <PinBadge x={74} y={80} pin={2} active={activePin === 2} onSelect={onSelectPin} />
    <PinBadge x={432} y={76} pin={3} active={activePin === 3} onSelect={onSelectPin} />
  </>
);

export const renderExplorerScene = (activePin: number | null, onSelectPin?: (p: number) => void) => (
  <>
    <rect x={22} y={20} width={300} height={228} rx={9} fill="var(--kivi-surface)" stroke="var(--kivi-border)" />
    <text x={34} y={39} fontFamily={MONO} fontSize={9.2} fontWeight={700} fill="var(--kivi-ink)">
      MAP EXPLORER
    </text>
    <rect x={34} y={48} width={236} height={24} rx={5} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
    <text x={44} y={63} fontFamily={MONO} fontSize={8.5} fill="var(--kivi-muted)">
      Cari nama map atau offset...
    </text>
    <rect x={278} y={48} width={32} height={24} rx={5} fill="var(--kivi-accent)" fillOpacity={0.16} {...focusStroke(activePin, 2)} />
    <text x={294} y={64} textAnchor="middle" fontFamily={MONO} fontSize={12} fontWeight={700} fill="var(--kivi-ink)">
      +
    </text>

    <g>
      <rect x={34} y={80} width={276} height={28} rx={6} fill="var(--kivi-bg)" {...focusStroke(activePin, 1)} />
      {[
        { label: 'All 14', x: 38, w: 62, active: false },
        { label: '2D 8', x: 105, w: 56, active: true },
        { label: '1D 3', x: 166, w: 56, active: false },
        { label: '1x1 3', x: 227, w: 62, active: false },
      ].map((pill) => (
        <g key={pill.label}>
          <rect
            x={pill.x}
            y={84}
            width={pill.w}
            height={20}
            rx={4}
            fill={pill.active ? 'var(--kivi-accent)' : 'var(--kivi-elevated)'}
            stroke={pill.active ? 'var(--kivi-accent)' : 'var(--kivi-border)'}
            strokeWidth={0.8}
          />
          <text
            x={pill.x + pill.w / 2}
            y={97}
            textAnchor="middle"
            fontFamily={MONO}
            fontSize={8.2}
            fontWeight={700}
            fill={pill.active ? 'var(--kivi-accent-fg)' : 'var(--kivi-secondary)'}
          >
            {pill.label}
          </text>
        </g>
      ))}
    </g>

    <rect x={34} y={116} width={276} height={22} rx={4} fill="var(--kivi-accent)" fillOpacity={0.14} stroke="var(--kivi-accent)" strokeWidth={0.8} />
    <text x={42} y={130} fontFamily={MONO} fontSize={8.5} fontWeight={600} fill="var(--kivi-ink)">
      Main Fuel Injection
    </text>
    <text x={300} y={130} textAnchor="end" fontFamily={MONO} fontSize={8} fontWeight={700} fill="var(--kivi-ink)">
      16x16
    </text>
    <text x={42} y={152} fontFamily={MONO} fontSize={8.5} fill="var(--kivi-secondary)">
      Ignition Advance High
    </text>
    <text x={300} y={152} textAnchor="end" fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">
      16x12
    </text>
    <text x={42} y={171} fontFamily={MONO} fontSize={8.5} fill="var(--kivi-secondary)">
      RPM Soft Cut Limit
    </text>
    <text x={300} y={171} textAnchor="end" fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">
      1x1
    </text>

    <g>
      <rect x={34} y={182} width={276} height={56} rx={6} fill="var(--kivi-warn)" fillOpacity={0.1} {...focusStroke(activePin, 3)} />
      <text x={42} y={197} fontFamily={MONO} fontSize={8.3} fontWeight={700} fill="var(--kivi-ink)">
        POTENTIAL MAPS (13)
      </text>
      <text x={42} y={214} fontFamily={MONO} fontSize={8.2} fill="var(--kivi-ink)">
        0x000060  Candidate 8x12
      </text>
      <text x={42} y={229} fontFamily={MONO} fontSize={8.2} fill="var(--kivi-secondary)">
        0x000180  Candidate 16x12
      </text>
    </g>

    <rect x={338} y={20} width={200} height={114} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 2)} />
    <text x={352} y={40} fontFamily={MONO} fontSize={8.8} fontWeight={700} fill="var(--kivi-ink)">
      + TAMBAH DEFINISI
    </text>
    {['Create New Table 2D/3D', 'Create 1D Axis', 'Create 1x1 Scalar', 'Create Bitmask Flag'].map((opt, idx) => (
      <g key={opt}>
        <rect x={350} y={48 + idx * 20} width={176} height={16} rx={4} fill="var(--kivi-bg)" stroke="var(--kivi-border)" strokeWidth={0.7} />
        <text x={358} y={59 + idx * 20} fontFamily={MONO} fontSize={8} fill="var(--kivi-secondary)">
          {opt}
        </text>
      </g>
    ))}

    <rect x={338} y={146} width={200} height={102} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 3)} />
    <text x={352} y={166} fontFamily={MONO} fontSize={8.8} fontWeight={700} fill="var(--kivi-warn)">
      TINJAU KANDIDAT SCAN
    </text>
    <text x={352} y={185} fontFamily={MONO} fontSize={8} fill="var(--kivi-secondary)">
      Klik item amber di Explorer
    </text>
    <text x={352} y={199} fontFamily={MONO} fontSize={8} fill="var(--kivi-secondary)">
      untuk lompat ke offset ROM,
    </text>
    <text x={352} y={213} fontFamily={MONO} fontSize={8} fill="var(--kivi-secondary)">
      lalu pilih Promote Map.
    </text>

    <PinBadge x={306} y={80} pin={1} active={activePin === 1} onSelect={onSelectPin} />
    <PinBadge x={320} y={46} pin={2} active={activePin === 2} onSelect={onSelectPin} />
    <PinBadge x={306} y={184} pin={3} active={activePin === 3} onSelect={onSelectPin} />
  </>
);

export const renderTablesScene = (activePin: number | null, onSelectPin?: (p: number) => void) => (
  <>
    <g>
      <rect x={22} y={20} width={516} height={30} rx={7} fill="var(--kivi-elevated)" {...focusStroke(activePin, 2)} />
      {['+ Add', '- Sub', '% Scale', '= Set', 'S Smooth', 'I Bilinear', 'Ctrl+\\ Split 3D'].map((btn, i) => (
        <g key={btn}>
          <rect
            x={30 + i * 71}
            y={25}
            width={65}
            height={20}
            rx={4}
            fill={i === 4 ? 'var(--kivi-accent)' : 'var(--kivi-surface)'}
            fillOpacity={i === 4 ? 0.16 : 1}
            stroke={i === 4 ? 'var(--kivi-accent)' : 'var(--kivi-border)'}
            strokeWidth={0.8}
          />
          <text x={62 + i * 71} y={38} textAnchor="middle" fontFamily={MONO} fontSize={7.8} fontWeight={600} fill="var(--kivi-ink)">
            {btn}
          </text>
        </g>
      ))}
    </g>

    <g>
      <rect x={22} y={62} width={312} height={186} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 1)} />
      <text x={34} y={80} fontFamily={MONO} fontSize={8.8} fontWeight={700} fill="var(--kivi-ink)">
        2D HEATMAP MATRIX (RPM x TPS)
      </text>
      {['2000', '4000', '6000', '8000', '9500'].map((col, c) => (
        <text key={col} x={102 + c * 46} y={99} textAnchor="middle" fontFamily={MONO} fontSize={7.8} fill="var(--kivi-muted)">
          {col}
        </text>
      ))}
      {['20%', '50%', '80%', '100%'].map((rowLabel, r) => (
        <g key={rowLabel}>
          <text x={36} y={121 + r * 31} fontFamily={MONO} fontSize={8} fill="var(--kivi-muted)">
            {rowLabel}
          </text>
          {['14.6', '13.8', '13.1', '12.6', '12.3'].map((val, c) => (
            <g key={c}>
              <rect
                x={80 + c * 46}
                y={106 + r * 31}
                width={43}
                height={25}
                rx={4}
                fill={c >= 3 && r >= 2 ? 'var(--kivi-warn)' : 'var(--kivi-accent)'}
                fillOpacity={0.1 + (c + r) * 0.032}
                stroke="var(--kivi-border)"
                strokeWidth={0.7}
              />
              <text x={101 + c * 46} y={122 + r * 31} textAnchor="middle" fontFamily={MONO} fontSize={8.2} fontWeight={600} fill="var(--kivi-ink)">
                {val}
              </text>
            </g>
          ))}
        </g>
      ))}
      <rect x={170} y={135} width={140} height={60} rx={5} fill="none" stroke="var(--kivi-accent)" strokeWidth={1.8} />
    </g>

    <g>
      <rect x={346} y={62} width={192} height={186} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 3)} />
      <text x={358} y={80} fontFamily={MONO} fontSize={8.8} fontWeight={700} fill="var(--kivi-ink)">
        3D SURFACE &amp; 1D CURVE
      </text>
      <path d="M 366 158 L 418 118 L 468 124 L 516 96" fill="none" stroke="var(--kivi-accent)" strokeWidth={1.6} />
      <path d="M 366 174 L 418 136 L 468 140 L 516 112" fill="none" stroke="var(--kivi-ok)" strokeWidth={1.5} />
      <path d="M 366 158 L 366 174 M 418 118 L 418 136 M 468 124 L 468 140 M 516 96 L 516 112" stroke="var(--kivi-border-strong)" strokeWidth={1} />
      <rect x={358} y={190} width={168} height={46} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={366} y={205} fontFamily={MONO} fontSize={7.8} fill="var(--kivi-secondary)">
        1D Curve: Stock (putus)
      </text>
      <path d="M 368 225 Q 422 212 514 210" fill="none" stroke="var(--kivi-muted)" strokeWidth={1.2} strokeDasharray="3 3" />
      <path d="M 368 228 Q 422 204 514 202" fill="none" stroke="var(--kivi-accent)" strokeWidth={1.8} />
    </g>

    <PinBadge x={326} y={66} pin={1} active={activePin === 1} onSelect={onSelectPin} />
    <PinBadge x={532} y={22} pin={2} active={activePin === 2} onSelect={onSelectPin} />
    <PinBadge x={526} y={76} pin={3} active={activePin === 3} onSelect={onSelectPin} />
  </>
);

export const renderScalarFlagScene = (activePin: number | null, onSelectPin?: (p: number) => void) => (
  <>
    <g>
      <rect x={22} y={20} width={248} height={124} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 1)} />
      <text x={36} y={39} fontFamily={MONO} fontSize={8.5} fontWeight={700} fill="var(--kivi-muted)">
        CALIBRATED PARAMETER VALUE
      </text>
      <rect x={36} y={48} width={112} height={46} rx={7} fill="var(--kivi-bg)" stroke="var(--kivi-accent)" strokeWidth={1.2} />
      <text x={92} y={79} textAnchor="middle" fontFamily={MONO} fontSize={23} fontWeight={700} fill="var(--kivi-ink)">
        129
      </text>
      <text x={160} y={64} fontFamily={MONO} fontSize={8.2} fill="var(--kivi-muted)">RAW HEX</text>
      <text x={160} y={77} fontFamily={MONO} fontSize={10} fontWeight={700} fill="var(--kivi-accent)">0x81</text>
      <text x={160} y={96} fontFamily={MONO} fontSize={8.2} fill="var(--kivi-muted)">BINARY</text>
      <text x={160} y={109} fontFamily={MONO} fontSize={9.5} fontWeight={600} fill="var(--kivi-ink)">10000001</text>
      <text x={36} y={132} fontFamily={MONO} fontSize={8} fill="var(--kivi-secondary)">
        Edit Decimal / Hex / Binary sinkron
      </text>
    </g>

    <g>
      <rect x={282} y={20} width={256} height={124} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 3)} />
      <rect x={442} y={32} width={82} height={24} rx={5} fill="var(--kivi-elevated)" stroke="var(--kivi-border-strong)" />
      <text x={483} y={47} textAnchor="middle" fontFamily={MONO} fontSize={8.2} fontWeight={700} fill="var(--kivi-ink)">
        Grid View
      </text>
      <text x={296} y={47} fontFamily={MONO} fontSize={8.8} fontWeight={700} fill="var(--kivi-ink)">
        STOCK COMPARISON
      </text>
      <rect x={296} y={64} width={228} height={32} rx={6} fill="var(--kivi-bg)" stroke="var(--kivi-border)" />
      <text x={306} y={83} fontFamily={MONO} fontSize={8.5} fill="var(--kivi-secondary)">Stock: 0 bitmask</text>
      <text x={514} y={83} textAnchor="end" fontFamily={MONO} fontSize={8.5} fontWeight={700} fill="var(--kivi-warn)">Delta +129</text>
      <text x={296} y={120} fontFamily={MONO} fontSize={8.2} fill="var(--kivi-muted)">
        Undo (Ctrl+Z) / Redo (Ctrl+Y) tersedia
      </text>
    </g>

    <g>
      <rect x={22} y={156} width={516} height={92} rx={9} fill="var(--kivi-surface)" {...focusStroke(activePin, 2)} />
      <text x={36} y={175} fontFamily={MONO} fontSize={8.8} fontWeight={700} fill="var(--kivi-ink)">
        BITMASK SWITCHES (8-BIT)
      </text>
      <text x={524} y={175} textAnchor="end" fontFamily={MONO} fontSize={8.5} fontWeight={700} fill="var(--kivi-accent)">
        2/8 ON
      </text>
      {[7, 6, 5, 4, 3, 2, 1, 0].map((bit, idx) => {
        const isOn = bit === 7 || bit === 0;
        const bx = 36 + idx * 62;
        return (
          <g key={bit}>
            <rect
              x={bx}
              y={186}
              width={56}
              height={48}
              rx={6}
              fill={isOn ? 'var(--kivi-accent)' : 'var(--kivi-bg)'}
              fillOpacity={isOn ? 0.14 : 1}
              stroke={isOn ? 'var(--kivi-accent)' : 'var(--kivi-border)'}
              strokeWidth={isOn ? 1.3 : 0.9}
            />
            <text x={bx + 28} y={204} textAnchor="middle" fontFamily={MONO} fontSize={8.2} fontWeight={700} fill="var(--kivi-ink)">
              Bit {bit}
            </text>
            <text x={bx + 28} y={222} textAnchor="middle" fontFamily={MONO} fontSize={8.5} fontWeight={700} fill={isOn ? 'var(--kivi-ink)' : 'var(--kivi-muted)'}>
              {isOn ? 'ON' : 'OFF'}
            </text>
          </g>
        );
      })}
    </g>

    <PinBadge x={256} y={38} pin={1} active={activePin === 1} onSelect={onSelectPin} />
    <PinBadge x={454} y={170} pin={2} active={activePin === 2} onSelect={onSelectPin} />
    <PinBadge x={532} y={24} pin={3} active={activePin === 3} onSelect={onSelectPin} />
  </>
);
