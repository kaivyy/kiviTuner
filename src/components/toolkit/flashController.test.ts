/**
 * Tests for the ECU dump/flash controller.
 *
 * The controller is the only place the UI touches the flash bridge, so these
 * tests pin the two things that matter: a `null` from the bridge must never be
 * presented as success, and a mock device must always be flagged.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  runListFlashDevices,
  runDumpEcu,
  runFlashEcu,
  summariseFlashResult,
  describeDeviceRisk,
  type FlashDeviceView,
} from './flashController.ts';

const mockDevice: FlashDeviceView = {
  id: 'attached',
  name: 'Mock Bench',
  kind: 'mock',
  transport: 'InMemory',
  isMock: true,
};

const realDevice: FlashDeviceView = {
  id: 'attached',
  name: 'Scanmatik SM2',
  kind: 'j2534',
  transport: 'Usb',
  isMock: false,
};

// ---------------------------------------------------------------------------
// Device listing
// ---------------------------------------------------------------------------

test('listing devices reports no-backend when the bridge returns null', async () => {
  const state = await runListFlashDevices(async () => null);
  assert.equal(state.kind, 'no-backend');
});

test('listing devices returns the devices when the bridge answers', async () => {
  const state = await runListFlashDevices(async () => [mockDevice]);
  assert.equal(state.kind, 'ok');
  if (state.kind === 'ok') {
    assert.equal(state.data.length, 1);
    assert.equal(state.data[0].name, 'Mock Bench');
  }
});

test('listing devices reports an empty list distinctly from no-backend', async () => {
  const state = await runListFlashDevices(async () => []);
  assert.equal(state.kind, 'ok');
  if (state.kind === 'ok') {
    assert.equal(state.data.length, 0);
  }
});

// ---------------------------------------------------------------------------
// Dump
// ---------------------------------------------------------------------------

test('a dump reports no-backend when the bridge returns null', async () => {
  const state = await runDumpEcu(async () => null, 0x0000, 0x100, 0x100);
  assert.equal(state.kind, 'no-backend');
});

test('a dump reports ok with the bytes', async () => {
  const state = await runDumpEcu(
    async () => ({ baseAddress: 0x0000, bytes: [1, 2, 3], blocksRead: 1 }),
    0x0000,
    3,
    3
  );
  assert.equal(state.kind, 'ok');
  if (state.kind === 'ok') {
    assert.deepEqual(state.data.bytes, [1, 2, 3]);
    assert.equal(state.data.blocksRead, 1);
  }
});

test('a dump refuses a zero-length request before calling the bridge', async () => {
  let called = false;
  const state = await runDumpEcu(
    async () => {
      called = true;
      return null;
    },
    0x0000,
    0,
    0x100
  );
  assert.equal(state.kind, 'error');
  assert.equal(called, false, 'the bridge must not be called for an invalid request');
});

test('a dump refuses a zero block size before calling the bridge', async () => {
  let called = false;
  const state = await runDumpEcu(
    async () => {
      called = true;
      return null;
    },
    0x0000,
    0x100,
    0
  );
  assert.equal(state.kind, 'error');
  assert.equal(called, false);
});

// ---------------------------------------------------------------------------
// Flash
// ---------------------------------------------------------------------------

test('a flash reports no-backend when the bridge returns null', async () => {
  const state = await runFlashEcu(async () => null, 0x0000, new Uint8Array([1, 2]), 0x100);
  assert.equal(state.kind, 'no-backend');
});

test('a flash reports ok when verified', async () => {
  const state = await runFlashEcu(
    async () => ({ verified: true, bytesWritten: 2, blocksWritten: 1 }),
    0x0000,
    new Uint8Array([1, 2]),
    0x100
  );
  assert.equal(state.kind, 'ok');
  if (state.kind === 'ok') {
    assert.equal(state.data.verified, true);
  }
});

test('a flash refuses an empty image before calling the bridge', async () => {
  let called = false;
  const state = await runFlashEcu(
    async () => {
      called = true;
      return null;
    },
    0x0000,
    new Uint8Array(0),
    0x100
  );
  assert.equal(state.kind, 'error');
  assert.equal(called, false);
});

// ---------------------------------------------------------------------------
// Summaries
// ---------------------------------------------------------------------------

test('a verified flash summarises as success', () => {
  const text = summariseFlashResult({ verified: true, bytesWritten: 0x100, blocksWritten: 1 });
  assert.match(text, /verified/i);
  assert.match(text, /256/);
});

test('an unverified flash summarises as a failure, not a success', () => {
  const text = summariseFlashResult({ verified: false, bytesWritten: 0x100, blocksWritten: 1 });
  assert.match(text, /fail|not verified|unverified/i);
  assert.doesNotMatch(text, /^verified/i);
});

// ---------------------------------------------------------------------------
// Risk labelling
// ---------------------------------------------------------------------------

test('a mock device is labelled as a test double', () => {
  const risk = describeDeviceRisk(mockDevice);
  assert.match(risk.level, /test|mock/i);
});

test('a real j2534 device is labelled as hardware', () => {
  const risk = describeDeviceRisk(realDevice);
  assert.match(risk.level, /hardware|real/i);
});

test('a real device warning mentions the ECU', () => {
  const risk = describeDeviceRisk(realDevice);
  assert.match(risk.message, /ECU/);
});

// ---------------------------------------------------------------------------
// J2534 driver discovery
// ---------------------------------------------------------------------------

test('discovering drivers reports no-backend when the bridge returns null', async () => {
  const { runListJ2534Drivers } = await import('./flashController.ts');
  const state = await runListJ2534Drivers(async () => null);
  assert.equal(state.kind, 'no-backend');
});

test('discovering drivers reports an empty list distinctly from no-backend', async () => {
  const { runListJ2534Drivers } = await import('./flashController.ts');
  const state = await runListJ2534Drivers(async () => []);
  assert.equal(state.kind, 'ok');
  if (state.kind === 'ok') assert.equal(state.data.length, 0);
});

test('an unloadable driver is described as unusable, not as ready', async () => {
  const { describeDriverUsability } = await import('./flashController.ts');
  const text = describeDriverUsability({
    name: 'smj2534.dll',
    path: '/x/smj2534.dll',
    score: 100,
    loadable: false,
  });
  assert.match(text, /cannot be loaded/i);
  assert.doesNotMatch(text, /ready/i);
});

test('a loadable driver is described as ready', async () => {
  const { describeDriverUsability } = await import('./flashController.ts');
  const text = describeDriverUsability({
    name: 'smj2534.dll',
    path: '/x/smj2534.dll',
    score: 100,
    loadable: true,
  });
  assert.match(text, /ready/i);
});

test('a 32-bit driver flagged via_bridge describes bridge readiness cleanly', async () => {
  const { describeDriverUsability } = await import('./flashController.ts');
  const text = describeDriverUsability({
    name: 'op20pt32.dll',
    path: 'C:\\Windows\\SysWOW64\\op20pt32.dll',
    score: 100,
    loadable: true,
    via_bridge: true,
    arch_label: 'PE x86 (32-bit DLL)',
  });
  assert.match(text, /32-bit J2534 bridge/i);
  assert.match(text, /ready to open/i);
});

// ---------------------------------------------------------------------------
// Transport selection
// ---------------------------------------------------------------------------

test('a successful OpenPort 2.0 attach reports openport2 transport', async () => {
  const { runAttachOpenPort2 } = await import('./flashController.ts');
  const s = await runAttachOpenPort2('/dev/ttyACM0', async () => null);
  assert.equal(s.choice, 'openport2');
  assert.equal(s.error, null);
});

test('attaching OpenPort 2.0 refuses an empty port path', async () => {
  const { runAttachOpenPort2 } = await import('./flashController.ts');
  let called = false;
  const s = await runAttachOpenPort2('  ', async () => {
    called = true;
    return null;
  });
  assert.equal(s.choice, null);
  assert.equal(called, false);
});

test('a successful DoIP attach reports doip transport', async () => {
  const { runAttachDoIp } = await import('./flashController.ts');
  const s = await runAttachDoIp('192.168.0.10:13400', async () => null);
  assert.equal(s.choice, 'doip');
  assert.equal(s.error, null);
});

test('a successful SocketCAN attach reports socketcan transport', async () => {
  const { runAttachSocketCan } = await import('./flashController.ts');
  const s = await runAttachSocketCan('can0', async () => null);
  assert.equal(s.choice, 'socketcan');
  assert.equal(s.error, null);
});

test('a successful J2534 attach reports the j2534 transport', async () => {
  const { runAttachJ2534 } = await import('./flashController.ts');
  const s = await runAttachJ2534('/x/smj2534.dll', async () => null);
  assert.equal(s.choice, 'j2534');
  assert.equal(s.error, null);
});

test('a failed J2534 attach keeps the error text', async () => {
  const { runAttachJ2534 } = await import('./flashController.ts');
  const s = await runAttachJ2534('/x/bad.dll', async () => 'could not open driver');
  assert.equal(s.choice, null);
  assert.equal(s.error, 'could not open driver');
});

test('an empty driver path is refused before calling the bridge', async () => {
  const { runAttachJ2534 } = await import('./flashController.ts');
  let called = false;
  const s = await runAttachJ2534('  ', async () => {
    called = true;
    return null;
  });
  assert.equal(called, false);
  assert.match(s.error as string, /required/i);
});

test('a serial transport cannot flash', async () => {
  const { transportCanFlash } = await import('./flashController.ts');
  assert.equal(transportCanFlash('serial'), false);
});

test('a j2534 transport can flash', async () => {
  const { transportCanFlash } = await import('./flashController.ts');
  assert.equal(transportCanFlash('j2534'), true);
});

test('an openport2, kline_serial, doip, or socketcan transport can flash while elm_wifi cannot', async () => {
  const { transportCanFlash, describeTransportCapability } = await import('./flashController.ts');
  assert.equal(transportCanFlash('openport2'), true);
  assert.equal(transportCanFlash('kline_serial'), true);
  assert.equal(transportCanFlash('doip'), true);
  assert.equal(transportCanFlash('socketcan'), true);
  assert.equal(transportCanFlash('elm_wifi'), false);
  assert.match(describeTransportCapability('elm_wifi'), /cannot flash/i);
  assert.match(describeTransportCapability('kline_serial'), /k-line/i);
});

test('no transport means no flashing', async () => {
  const { transportCanFlash } = await import('./flashController.ts');
  assert.equal(transportCanFlash(null), false);
});

test('a serial transport is described as unable to flash', async () => {
  const { describeTransportCapability } = await import('./flashController.ts');
  const text = describeTransportCapability('serial');
  assert.match(text, /cannot flash/i);
});

test('a UDS dump reports no-backend when the bridge returns null', async () => {
  const { runDumpUds23 } = await import('./flashController.ts');
  const s = await runDumpUds23(async () => null, 0, 0x100, 0x40);
  assert.equal(s.kind, 'no-backend');
});

test('a UDS dump refuses a zero length before the bridge', async () => {
  const { runDumpUds23 } = await import('./flashController.ts');
  let called = false;
  const s = await runDumpUds23(
    async () => {
      called = true;
      return null;
    },
    0,
    0,
    0x40
  );
  assert.equal(called, false);
  assert.equal(s.kind, 'error');
});

test('a UDS dump refuses a block larger than the 2-byte size field', async () => {
  const { runDumpUds23 } = await import('./flashController.ts');
  let called = false;
  const s = await runDumpUds23(
    async () => {
      called = true;
      return null;
    },
    0,
    0x100,
    0x10000
  );
  assert.equal(called, false);
  assert.match((s as { message: string }).message, /65535/);
});

test('a successful UDS dump reports ok', async () => {
  const { runDumpUds23 } = await import('./flashController.ts');
  const s = await runDumpUds23(
    async () => ({ baseAddress: 0, bytes: [1, 2, 3], blocksRead: 1, pendingRetries: 0 }),
    0,
    3,
    3
  );
  assert.equal(s.kind, 'ok');
});

test('the dump summary names the byte count and address', async () => {
  const { summariseUdsDump } = await import('./flashController.ts');
  const text = summariseUdsDump({ baseAddress: 0, bytes: [1, 2], blocksRead: 1, pendingRetries: 0 });
  assert.match(text, /2 bytes/);
  assert.match(text, /00000000/);
});

test('describeDriverUsability surfaces binary architecture and mismatch reason cleanly', async () => {
  const { describeDriverUsability } = await import('./flashController.ts');
  const mismatch = describeDriverUsability({
    name: 'op20pt32.dll',
    path: 'C:\\Program Files (x86)\\op20pt32.dll',
    score: 100,
    loadable: false,
    arch_label: 'PE x86 (32-bit DLL)',
    reason: '32-bit x86 DLL cannot load inside a 64-bit app; use native OpenPort 2.0 USB-CDC.',
  });
  assert.match(mismatch, /PE x86/);
  assert.match(mismatch, /cannot be loaded: 32-bit x86/);

  const ok = describeDriverUsability({
    name: 'libj2534_openport2.so',
    path: '/usr/local/lib/libj2534_openport2.so',
    score: 100,
    loadable: true,
    arch_label: 'ELF x64 (64-bit .so)',
    reason: null,
  });
  assert.match(ok, /ELF x64/);
  assert.match(ok, /ready/i);
});

test('runAutoConnectTransport distinguishes ok, no-backend, and error', async () => {
  const { runAutoConnectTransport } = await import('./flashController.ts');
  assert.equal((await runAutoConnectTransport(async () => null)).kind, 'no-backend');
  assert.equal(
    (
      await runAutoConnectTransport(async () => {
        throw new Error('probe crashed');
      })
    ).kind,
    'error'
  );
  const ok = await runAutoConnectTransport(async () => ({
    connected: true,
    transport: 'openport2',
    device_name: 'OpenPort Clone (/dev/ttyACM0)',
    target: '/dev/ttyACM0',
    attempts: ['Detected OpenPort clone'],
  }));
  assert.equal(ok.kind, 'ok');
  if (ok.kind === 'ok') {
    assert.equal(ok.data.connected, true);
    assert.equal(ok.data.transport, 'openport2');
  }
});

test('quick diagnostics controller wrappers handle ok, no-backend, and errors cleanly', async () => {
  const {
    runClearDtcs,
    runEcuReset,
    runReadDtcs,
    runReadEcuInfo,
    summariseDtcRead,
    summariseEcuInfo,
  } = await import('./flashController.ts');

  assert.equal((await runReadEcuInfo(async () => null)).kind, 'no-backend');
  assert.equal((await runReadDtcs(async () => null)).kind, 'no-backend');
  assert.equal((await runClearDtcs(async () => null)).kind, 'no-backend');
  assert.equal((await runEcuReset('soft', async () => null)).kind, 'no-backend');

  const errRes = await runReadDtcs(async () => {
    throw new Error('NO DATA');
  });
  assert.equal(errRes.kind, 'error');

  const infoOk = await runReadEcuInfo(async () => ({
    battery_mv: 12740,
    vin: 'JH2PC4000MK123456',
    cal_id: '38770-K15-901',
    cvn: 'AABBCCDD',
    protocol: 'Iso15765',
    notes: [],
  }));
  assert.equal(infoOk.kind, 'ok');
  if (infoOk.kind === 'ok') {
    const summary = summariseEcuInfo(infoOk.data);
    assert.match(summary, /12\.74V/);
    assert.match(summary, /JH2PC4000MK123456/);
  }

  const dtcOk = await runReadDtcs(async () => ({
    dtcs: [
      { code: 'P0171', status_mask: 8, status_label: 'confirmed', source: 'obd_mode03' },
    ],
    protocol_used: 'OBD-II Mode 03/07',
    notes: [],
  }));
  assert.equal(dtcOk.kind, 'ok');
  if (dtcOk.kind === 'ok') {
    assert.match(summariseDtcRead(dtcOk.data), /1 trouble code/);
  }
});

test('Phase 1 ELM Wi-Fi, K-Line, configured serial, protocol override, and capability probe actions validate inputs cleanly', async () => {
  const {
    runAttachSerialConfigured,
    runAttachElmWifi,
    runAttachKLineSerial,
    runReconfigureTransport,
    runProbeElmCapabilities,
    runSendRawDiagCommand,
  } = await import('./flashController.ts');

  const wifiErr = await runAttachElmWifi('   ', 'auto');
  assert.match(wifiErr.error as string, /host:port/i);

  const wifiOk = await runAttachElmWifi('192.168.0.10:35000', 'honda_kline_experimental', async () => null);
  assert.equal(wifiOk.choice, 'elm_wifi');
  assert.equal(wifiOk.error, null);

  const klineErr = await runAttachKLineSerial('', 10400);
  assert.match(klineErr.error as string, /required/i);

  const klineOk = await runAttachKLineSerial('/dev/ttyUSB0', 10400, async () => null);
  assert.equal(klineOk.choice, 'kline_serial');
  assert.equal(klineOk.error, null);

  let seenArgs: unknown[] = [];
  const serOk = await runAttachSerialConfigured(
    '/dev/rfcomm0',
    undefined,
    'kwp_fast_init',
    async (p, b, prof) => {
      seenArgs = [p, b, prof];
      return null;
    }
  );
  assert.equal(serOk.choice, 'serial');
  assert.deepEqual(seenArgs, ['/dev/rfcomm0', undefined, 'kwp_fast_init']);

  assert.match(
    (await runReconfigureTransport('iso14230', 0, undefined, undefined, true)) as string,
    /positive integer/i
  );
  assert.equal(
    await runReconfigureTransport('iso14230', 10400, 0x7e0, 0x7e8, true, async () => null),
    null
  );

  const blankCmd = await runSendRawDiagCommand('   ');
  assert.equal(blankCmd.kind, 'error');

  const okCmd = await runSendRawDiagCommand('ATI', async () => 'ELM327 v1.5');
  assert.equal(okCmd.kind, 'ok');
  if (okCmd.kind === 'ok') {
    assert.equal(okCmd.data, 'ELM327 v1.5');
  }

  const okCaps = await runProbeElmCapabilities(async () => ({
    adapter_banner: 'ELM327 v1.5',
    chip_tier: 'pic18_v15_compatible',
    active_protocol: 'A6',
    supports_atsh: true,
    supports_atcaf0: true,
    supports_kline_atsp5: true,
    supports_atbi: true,
    supports_st_commands: false,
    warnings: [],
  }));
  assert.equal(okCaps.kind, 'ok');
  if (okCaps.kind === 'ok') {
    assert.equal(okCaps.data.chip_tier, 'pic18_v15_compatible');
  }
});
