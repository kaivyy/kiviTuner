import React from 'react';

export const MONO = 'var(--kivi-font-mono)';

export const focusStroke = (activePin: number | null, pin: number) => ({
  stroke: activePin === pin ? 'var(--kivi-accent)' : 'var(--kivi-border)',
  strokeWidth: activePin === pin ? 1.7 : 1,
});

interface PinBadgeProps {
  x: number;
  y: number;
  pin: number;
  active: boolean;
  onSelect?: (pin: number) => void;
}

export const PinBadge: React.FC<PinBadgeProps> = ({ x, y, pin, active, onSelect }) => (
  <g
    transform={`translate(${x}, ${y})`}
    onClick={onSelect ? () => onSelect(pin) : undefined}
    style={{ cursor: onSelect ? 'pointer' : 'default' }}
  >
    <circle r={12} fill="transparent" />
    <circle
      r={9.5}
      fill={active ? 'var(--kivi-accent)' : 'var(--kivi-elevated)'}
      stroke="var(--kivi-accent)"
      strokeWidth={active ? 1.8 : 1.2}
    />
    <text
      x={0}
      y={3.5}
      textAnchor="middle"
      fontFamily={MONO}
      fontSize={9.5}
      fontWeight={700}
      fill={active ? 'var(--kivi-accent-fg)' : 'var(--kivi-ink)'}
    >
      {pin}
    </text>
  </g>
);
