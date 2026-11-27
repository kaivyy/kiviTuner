//! Tests for the ECU dump/flash bridge.
//!
//! The bridge must never throw into the UI and must never silently claim success
//! when the backend is absent. In a browser there is no flash engine, so every
//! call returns `null` — the caller is expected to disable the control.

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  bridgeListFlashDevices,
  bridgeDumpEcu,
  bridgeFlashEcu,
} from './flashBridge.ts';

test('listing devices returns null in a browser (no backend)', async () => {
  const result = await bridgeListFlashDevices();
  assert.equal(result, null);
});

test('dumping returns null in a browser (no backend)', async () => {
  const result = await bridgeDumpEcu(0x0000, 0x1000, 0x100);
  assert.equal(result, null);
});

test('flashing returns null in a browser (no backend)', async () => {
  const result = await bridgeFlashEcu(0x0000, new Uint8Array([0x01, 0x02]), 0x100);
  assert.equal(result, null);
});

test('dumping never throws, even with nonsense arguments', async () => {
  const result = await bridgeDumpEcu(0xffffffff, 0, 0);
  assert.equal(result, null);
});

test('flashing never throws with an empty image', async () => {
  const result = await bridgeFlashEcu(0x0000, new Uint8Array(0), 0x100);
  assert.equal(result, null);
});

test('listing J2534 drivers returns null in a browser (no backend)', async () => {
  const { bridgeListJ2534Drivers } = await import('./flashBridge.ts');
  assert.equal(await bridgeListJ2534Drivers(), null);
});

test('listing serial ports and attaching OpenPort2/DoIP/SocketCAN report no backend in a browser', async () => {
  const {
    bridgeListSerialPorts,
    bridgeAttachOpenPort2Transport,
    bridgeAttachDoIpTransport,
    bridgeAttachSocketCanTransport,
    bridgeDetachTransport,
  } = await import('./flashBridge.ts');
  assert.equal(await bridgeListSerialPorts(), null);
  assert.match((await bridgeAttachOpenPort2Transport('/dev/ttyACM0')) as string, /no desktop backend/i);
  assert.match((await bridgeAttachDoIpTransport('192.168.0.10:13400')) as string, /no desktop backend/i);
  assert.match((await bridgeAttachSocketCanTransport('can0')) as string, /no desktop backend/i);
  assert.match((await bridgeDetachTransport()) as string, /no desktop backend/i);
});

test('attaching a J2534 driver in a browser reports no backend', async () => {
  const { bridgeAttachJ2534Transport } = await import('./flashBridge.ts');
  const result = await bridgeAttachJ2534Transport('/x/smj2534.dll');
  assert.equal(typeof result, 'string');
  assert.match(result as string, /no desktop backend/i);
});

test('attaching a serial port in a browser reports no backend', async () => {
  const { bridgeAttachSerialTransport } = await import('./flashBridge.ts');
  const result = await bridgeAttachSerialTransport('/dev/ttyUSB0');
  assert.match(result as string, /no desktop backend/i);
});

test('the selected transport is null in a browser', async () => {
  const { bridgeSelectedTransport } = await import('./flashBridge.ts');
  assert.equal(await bridgeSelectedTransport(), null);
});

test('a UDS 0x23 dump returns null in a browser (no backend)', async () => {
  const { bridgeDumpEcuUds23 } = await import('./flashBridge.ts');
  assert.equal(await bridgeDumpEcuUds23(0x0000, 0x1000, 0x100), null);
});

test('a UDS 0x23 dump never throws with nonsense arguments', async () => {
  const { bridgeDumpEcuUds23 } = await import('./flashBridge.ts');
  assert.equal(await bridgeDumpEcuUds23(0xffffffff, 0, 0), null);
});

test('a streamed UDS 0x23 dump returns null in a browser (no backend)', async () => {
  const { bridgeDumpEcuUds23Stream } = await import('./flashBridge.ts');
  assert.equal(
    await bridgeDumpEcuUds23Stream(0x0000, 0x1000, 0x100, () => {}),
    null
  );
});

test('quick diagnostics bridges return null cleanly in a browser (no backend)', async () => {
  const {
    bridgeDiagReadEcuInfo,
    bridgeDiagReadDtcs,
    bridgeDiagClearDtcs,
    bridgeDiagEcuReset,
  } = await import('./flashBridge.ts');
  assert.equal(await bridgeDiagReadEcuInfo(), null);
  assert.equal(await bridgeDiagReadDtcs(), null);
  assert.equal(await bridgeDiagClearDtcs(), null);
  assert.equal(await bridgeDiagEcuReset('soft'), null);
});

test('auto-connecting flash transport returns null in a browser (no backend)', async () => {
  const { bridgeAutoConnectFlashTransport } = await import('./flashBridge.ts');
  assert.equal(await bridgeAutoConnectFlashTransport(), null);
});
