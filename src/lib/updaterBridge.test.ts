import test from 'node:test';
import assert from 'node:assert/strict';
import {
  APP_VERSION,
  bridgeCheckForUpdate,
  isNewerVersion,
  parseSemverTuple,
} from './updaterBridge.ts';

test('parseSemverTuple and isNewerVersion compare versions accurately', () => {
  assert.deepEqual(parseSemverTuple('v0.2.1'), [0, 2, 1, false]);
  assert.deepEqual(parseSemverTuple('1.0.0-beta.2'), [1, 0, 0, true]);
  assert.equal(isNewerVersion('0.1.0', 'v0.1.1'), true);
  assert.equal(isNewerVersion('0.1.0', '0.2.0'), true);
  assert.equal(isNewerVersion('0.2.0', '0.2.0'), false);
  assert.equal(isNewerVersion('0.3.0', 'v0.2.9'), false);
  assert.equal(isNewerVersion('0.2.0-rc.1', '0.2.0'), true);
});

test('bridgeCheckForUpdate parses desktop IPC update metadata cleanly', async () => {
  const info = await bridgeCheckForUpdate({
    isDesktop: () => true,
    invokeFn: (async () => ({
      update_available: true,
      current_version: '0.1.0',
      latest_version: '0.2.0',
      release_name: 'kiviTuner v0.2.0',
      release_notes: 'UI fixes and updater support',
      published_at: '2026-11-28T00:00:00Z',
      release_url: 'https://github.com/kaivyy/kiviTuner/releases/tag/v0.2.0',
      asset_name: 'kiviTuner_0.2.0_amd64.AppImage',
      asset_url: 'https://github.com/kaivyy/kiviTuner/releases/download/v0.2.0/kiviTuner_0.2.0_amd64.AppImage',
      asset_size: 42_000_000,
      sha256_url: 'https://github.com/kaivyy/kiviTuner/releases/download/v0.2.0/SHA256SUMS.txt',
      signed_updater_ready: false,
    })) as unknown as typeof import('@tauri-apps/api/core').invoke,
  });

  assert.equal(info.updateAvailable, true);
  assert.equal(info.latestVersion, '0.2.0');
  assert.equal(info.assetName, 'kiviTuner_0.2.0_amd64.AppImage');
});

test('bridgeCheckForUpdate handles web preview release JSON fallback cleanly', async () => {
  const info = await bridgeCheckForUpdate({
    isDesktop: () => false,
    fetchFn: (async () =>
      ({
        ok: true,
        status: 200,
        json: async () => ({
          tag_name: 'v9.9.9',
          name: 'kiviTuner v9.9.9',
          body: 'Release notes',
          html_url: 'https://github.com/kaivyy/kiviTuner/releases/tag/v9.9.9',
          draft: false,
          prerelease: false,
        }),
      }) as Response) as typeof fetch,
  });

  assert.equal(info.currentVersion, APP_VERSION);
  assert.equal(info.updateAvailable, true);
  assert.equal(info.latestVersion, '9.9.9');
});
