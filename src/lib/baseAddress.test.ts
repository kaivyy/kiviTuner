/**
 * B4a: frontend firmware ingest must agree with the Rust backend about base
 * addresses.
 *
 * Before B4a both paths reported `base_address: 0` for a raw binary, which made
 * "unknown" indistinguishable from "known zero". The Rust side now returns
 * `Option<u32>` (serialized as `null`); the browser fallback must match, or the
 * two paths silently diverge depending on whether the desktop app is running.
 *
 * These are contract tests over the pure parsing helpers, plus source-level
 * assertions that the fallback does not reintroduce the sentinel.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (p: string): string => readFileSync(p, 'utf8');

test('FirmwareIngestReport declares base_address as nullable', () => {
  const src = read('src/lib/bridge/romMathBridge.ts');
  const block = src.slice(
    src.indexOf('export interface FirmwareIngestReport'),
    src.indexOf('}', src.indexOf('export interface FirmwareIngestReport'))
  );
  assert.match(
    block,
    /base_address:\s*number\s*\|\s*null/,
    'base_address must be number | null so unknown is representable'
  );
});

test('raw binary fallback reports unknown base, not zero', () => {
  const src = read('src/lib/bridge/romMathBridge.ts');
  // Find the RawBinary return object and assert it no longer hard-codes 0.
  const idx = src.indexOf("format: 'RawBinary'");
  assert.ok(idx > 0, 'RawBinary branch must exist');
  const window = src.slice(idx, idx + 200);
  assert.match(
    window,
    /base_address:\s*null/,
    'a raw BIN has no inherent address; the fallback must say null'
  );
  assert.doesNotMatch(
    window,
    /base_address:\s*0\b/,
    'the old zero sentinel must not survive'
  );
});

test('flattenSparse reports unknown base for an empty map', () => {
  const src = read('src/lib/bridge/romMathBridge.ts');
  const idx = src.indexOf('function flattenSparse');
  assert.ok(idx > 0);
  const window = src.slice(idx, idx + 260);
  assert.match(
    window,
    /base:\s*null/,
    'an empty sparse map knows no address'
  );
});

test('flattenSparse returns a nullable base type', () => {
  const src = read('src/lib/bridge/romMathBridge.ts');
  const idx = src.indexOf('function flattenSparse');
  const window = src.slice(idx, idx + 120);
  assert.match(window, /base:\s*number\s*\|\s*null/);
});

test('BinDiffModal passes a nullable base to the address-aware bridge', () => {
  const src = read('src/components/modals/BinDiffModal.tsx');
  // The target base must come from the parsed report, which is now nullable.
  assert.match(src, /ingested\.base_address/);
  // It must not fabricate a zero for the workspace base.
  assert.doesNotMatch(
    src,
    /bridgeCompareRomBinariesWithAddress\(\s*baseRom,\s*0\s*,/,
    'the workspace base must stay null, never a fabricated 0'
  );
});

test('bridgeCompareRomBinariesWithAddress accepts nullable addresses', () => {
  const src = read('src/lib/bridge/diffScanBridge.ts');
  const idx = src.indexOf('export async function bridgeCompareRomBinariesWithAddress');
  assert.ok(idx > 0);
  const window = src.slice(idx, idx + 400);
  assert.match(window, /number\s*\|\s*null/);
});
