import type { Config } from 'tailwindcss';

const MONO_FONT_STACK = [
  'JetBrains Mono',
  'Fira Code',
  'Cascadia Mono',
  'ui-monospace',
  'SFMono-Regular',
  'Menlo',
  'Monaco',
  'Consolas',
  'monospace',
];

export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        darkBg: 'rgb(var(--kivi-bg-rgb) / <alpha-value>)',
        darkSurface: 'rgb(var(--kivi-surface-rgb) / <alpha-value>)',
        darkBorder: 'rgb(var(--kivi-border-rgb) / <alpha-value>)',
        kiviBg: 'rgb(var(--kivi-bg-rgb) / <alpha-value>)',
        kiviSurface: 'rgb(var(--kivi-surface-rgb) / <alpha-value>)',
        kiviElevated: 'rgb(var(--kivi-elevated-rgb) / <alpha-value>)',
        kiviBorder: 'rgb(var(--kivi-border-rgb) / <alpha-value>)',
        kiviBorderStrong: 'rgb(var(--kivi-border-strong-rgb) / <alpha-value>)',
        kiviInk: 'rgb(var(--kivi-ink-rgb) / <alpha-value>)',
        kiviSecondary: 'rgb(var(--kivi-secondary-rgb) / <alpha-value>)',
        kiviMuted: 'rgb(var(--kivi-muted-rgb) / <alpha-value>)',
        kiviAccent: 'rgb(var(--kivi-accent-rgb) / <alpha-value>)',
        kiviAccentFg: 'rgb(var(--kivi-accent-fg-rgb) / <alpha-value>)',
        kiviGreen: 'rgb(var(--kivi-ok-rgb) / <alpha-value>)',
        kiviWarn: 'rgb(var(--kivi-warn-rgb) / <alpha-value>)',
        kiviRed: 'rgb(var(--kivi-danger-rgb) / <alpha-value>)',
      },
      fontFamily: {
        sans: MONO_FONT_STACK,
        mono: MONO_FONT_STACK,
      },
    },
  },
  plugins: [],
} satisfies Config;
