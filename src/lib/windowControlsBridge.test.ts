import test from 'node:test';
import assert from 'node:assert/strict';
import {
  bridgeWindowClose,
  bridgeWindowIsMaximized,
  bridgeWindowMinimize,
  bridgeWindowStartDragging,
  bridgeWindowToggleMaximize,
  type DesktopWindowLike,
} from './windowControlsBridge.ts';

function fakeDesktopWindow(): { win: DesktopWindowLike; calls: string[]; maximized: { v: boolean }; fs: { v: boolean } } {
  const calls: string[] = [];
  const maximized = { v: false };
  const fs = { v: false };
  const win: DesktopWindowLike = {
    close: async () => {
      calls.push('close');
    },
    minimize: async () => {
      calls.push('minimize');
    },
    toggleMaximize: async () => {
      maximized.v = !maximized.v;
      calls.push(`toggleMaximize:${maximized.v}`);
    },
    isMaximized: async () => maximized.v,
    isFullscreen: async () => fs.v,
    setFullscreen: async (next: boolean) => {
      fs.v = next;
      calls.push(`setFullscreen:${next}`);
    },
    startDragging: async () => {
      calls.push('startDragging');
    },
  };
  return { win, calls, maximized, fs };
}

test('windowControlsBridge falls back safely in non-Tauri browser/node environments', async () => {
  assert.equal(await bridgeWindowClose({ isDesktop: () => false }), false);
  assert.equal(await bridgeWindowMinimize({ isDesktop: () => false }), false);
  assert.equal(await bridgeWindowToggleMaximize(false, { isDesktop: () => false }), false);
  assert.equal(await bridgeWindowIsMaximized({ isDesktop: () => false }), false);
  assert.equal(await bridgeWindowStartDragging({ isDesktop: () => false }), false);
});

test('windowControlsBridge delegates close, minimize, maximize, fullscreen, and drag on desktop', async () => {
  const { win, calls } = fakeDesktopWindow();
  const deps = { isDesktop: () => true, getWindow: () => win };

  assert.equal(await bridgeWindowMinimize(deps), true);
  assert.equal(await bridgeWindowToggleMaximize(false, deps), true);
  assert.equal(await bridgeWindowIsMaximized(deps), true);
  assert.equal(await bridgeWindowToggleMaximize(false, deps), false);
  assert.equal(await bridgeWindowToggleMaximize(true, deps), true);
  assert.equal(await bridgeWindowStartDragging(deps), true);
  assert.equal(await bridgeWindowClose(deps), true);

  assert.deepEqual(calls, [
    'minimize',
    'toggleMaximize:true',
    'toggleMaximize:false',
    'setFullscreen:true',
    'startDragging',
    'close',
  ]);
});

test('getStandaloneWindowId validates known ?window= params and rejects unknown values', async () => {
  const { getStandaloneWindowId, bridgeOpenToolWindow, bridgeSetWindowAlwaysOnTop } = await import(
    './windowControlsBridge.ts'
  );
  assert.equal(getStandaloneWindowId('?window=telemetry'), 'telemetry');
  assert.equal(getStandaloneWindowId('?window=guide'), 'guide');
  assert.equal(getStandaloneWindowId('window=map-props'), 'map-props');
  assert.equal(getStandaloneWindowId('?window=unknown-tool'), null);
  assert.equal(getStandaloneWindowId(''), null);

  // Browser fallback returns false cleanly
  assert.equal(
    await bridgeOpenToolWindow(
      {
        toolId: 'flash',
        title: 'Flash',
        width: 700,
        height: 700,
        minWidth: 420,
        minHeight: 460,
      },
      { isDesktop: () => false }
    ),
    false
  );
  assert.equal(await bridgeSetWindowAlwaysOnTop(true, { isDesktop: () => false }), false);

  // Desktop mode delegates IPC cleanly
  const invoked: Array<{ cmd: string; args: unknown }> = [];
  const fakeInvoke = (async (cmd: string, args?: unknown) => {
    invoked.push({ cmd, args });
    return cmd === 'set_current_window_always_on_top' ? true : 'kivi-flash';
  }) as unknown as typeof import('@tauri-apps/api/core').invoke;

  assert.equal(
    await bridgeOpenToolWindow(
      {
        toolId: 'flash',
        title: 'ECU Read & Flash',
        width: 700,
        height: 700,
        minWidth: 420,
        minHeight: 460,
      },
      { isDesktop: () => true, invokeFn: fakeInvoke }
    ),
    true
  );
  assert.equal(
    await bridgeSetWindowAlwaysOnTop(true, { isDesktop: () => true, invokeFn: fakeInvoke }),
    true
  );
  assert.equal(invoked[0].cmd, 'open_tool_window');
  assert.equal(invoked[1].cmd, 'set_current_window_always_on_top');
});

test('isMacDesktop identifies macOS desktop cleanly without leaking in browser or Windows/Linux', async () => {
  const { isMacDesktop } = await import('./windowControlsBridge.ts');
  assert.equal(isMacDesktop({ isDesktop: () => false, platform: 'MacIntel' }), false);
  assert.equal(isMacDesktop({ isDesktop: () => true, platform: 'Win32' }), false);
  assert.equal(isMacDesktop({ isDesktop: () => true, platform: 'Linux x86_64' }), false);
  assert.equal(isMacDesktop({ isDesktop: () => true, platform: 'MacIntel' }), true);
  assert.equal(
    isMacDesktop({ isDesktop: () => true, platform: '', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' }),
    true
  );
});
