/**
 * Wave 4 frontend contract tests.
 *
 * A bridge that exists but is never referenced by a component is not
 * integration. These tests assert both the command contract and the wiring.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (p: string): string => readFileSync(p, 'utf8');

// ---------------------------------------------------------------------------
// Command + bridge contract
// ---------------------------------------------------------------------------

test('all four Wave 4 commands are registered', () => {
  const lib = read('src-tauri/src/lib.rs');
  for (const cmd of [
    'scan_rom_heuristic_windows',
    'parse_vbf',
    'serialize_vbf',
    'translate_address',
  ]) {
    assert.match(lib, new RegExp(`commands::${cmd}\\b`), `${cmd} must be registered`);
  }
});

const BRIDGE_PATH = 'src/lib/bridge/diffScanBridge.ts';

test('bridges invoke the exact registered command names', () => {
  const src = read(BRIDGE_PATH);
  const pairs: Array<[string, string]> = [
    ['bridgeScanRomHeuristicWindows', 'scan_rom_heuristic_windows'],
    ['bridgeParseVbf', 'parse_vbf'],
    ['bridgeSerializeVbf', 'serialize_vbf'],
    ['bridgeTranslateAddress', 'translate_address'],
  ];
  for (const [bridge, cmd] of pairs) {
    assert.match(src, new RegExp(`function ${bridge}`), `${bridge} must exist`);
    assert.match(src, new RegExp(`invoke<[^>]*>\\('${cmd}'`), `${bridge} must invoke ${cmd}`);
  }
});

test('no Wave 4 bridge declares a bare any type', () => {
  const src = read(BRIDGE_PATH);
  const start = src.indexOf('Wave 4: VBF container');
  assert.ok(start > 0, 'Wave 4 section must exist');
  const section = src.slice(start);
  assert.doesNotMatch(section, /:\s*any\b/, 'Wave 4 bridges must not use any');
  assert.doesNotMatch(section, /\brequire\(/, 'no CommonJS require');
});

// ---------------------------------------------------------------------------
// Typed result shapes
// ---------------------------------------------------------------------------

test('translation result exposes the matched rule, not just the value', () => {
  const src = read(BRIDGE_PATH);
  const idx = src.indexOf('export interface AddressTranslationReport');
  const block = src.slice(idx, src.indexOf('}', idx) + 1);
  assert.match(block, /value:\s*number/);
  assert.match(block, /is_identity_fallback:\s*boolean/);
  assert.match(block, /matched_rule:\s*string\s*\|\s*null/);
});

test('the window-scan result is typed and carries the score fields', () => {
  const src = read(BRIDGE_PATH);
  const idx = src.indexOf('export interface DiscoveredWindowMap');
  const block = src.slice(idx, src.indexOf('}', idx) + 1);
  for (const field of ['offset', 'rows', 'cols', 'score', 'smoothness', 'suggested_name']) {
    assert.match(block, new RegExp(`${field}:`), `${field} must be present`);
  }
});

// ---------------------------------------------------------------------------
// Detector separation (mandatory — scores are not comparable)
// ---------------------------------------------------------------------------

test('the two scanners are documented as non-comparable', () => {
  const src = read(BRIDGE_PATH);
  const idx = src.indexOf('bridgeScanRomHeuristicWindows');
  const block = src.slice(idx - 900, idx + 200);
  assert.match(
    block,
    /not comparable|must not be combined|different scoring scale/i,
    'the bridge must warn that the two detectors score differently'
  );
});

test('the new scanner does not replace the existing one', () => {
  const src = read(BRIDGE_PATH);
  assert.match(src, /export async function bridgeScanRomHeuristics\b/);
  assert.match(src, /export async function bridgeScanRomHeuristicWindows\b/);
});

// ---------------------------------------------------------------------------
// Backend limitation surfacing
// ---------------------------------------------------------------------------

test('the VBF bridge states it is not validated against OEM files', () => {
  const src = read(BRIDGE_PATH);
  const idx = src.indexOf('export async function bridgeParseVbf');
  const block = src.slice(idx - 700, idx + 200);
  assert.match(block, /not validated against real OEM/i);
});

test('the VBF bridge does not fabricate a browser fallback', () => {
  const src = read(BRIDGE_PATH);
  const idx = src.indexOf('export async function bridgeParseVbf');
  const block = src.slice(idx, idx + 700);
  assert.match(
    block,
    /return null/,
    'without a backend the parser must return null rather than pretend'
  );
});
