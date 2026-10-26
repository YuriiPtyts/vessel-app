import { test, expect } from '@playwright/test';

// A presentation-contract test. Real extraction/decoding is exercised separately by Rust.
test('selects an actual format, receives native events, and controls playback', async ({ page }) => {
  await page.addInitScript(() => {
    const track = { id: 'fixture', title: 'A fixture song', artist: 'A fixture artist', durationSeconds: 90 };
    const formats = [
      { id: '251', codec: 'opus', container: 'webm', bitrateKbps: 129, sampleRateHz: 48000, channels: 2 },
      { id: '140', codec: 'mp4a.40.2', container: 'm4a', bitrateKbps: 128, sampleRateHz: 44100, channels: 2 },
    ];
    let snapshot = {
      status: 'idle', track: null, activeFormat: null, positionSeconds: 0, durationSeconds: null,
      volume: 40, error: null, videoTrackSelected: false, decodedSampleRateHz: null,
    };
    const callbacks = new Map();
    let listener;
    window.isTauri = true;
    window.__vesselCalls = [];
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) { const id = callbacks.size + 1; callbacks.set(id, callback); return id; },
      async invoke(command, args) {
        window.__vesselCalls.push({ command, args });
        switch (command) {
          case 'runtime_report': return { appVersion: 'test', platform: 'macos', architecture: 'aarch64', debugBuild: true, playbackDependenciesAvailable: true };
          case 'plugin:event|listen': listener = callbacks.get(args.handler); return 1;
          case 'plugin:event|unlisten': return;
          case 'playback_snapshot': return structuredClone(snapshot);
          case 'inspect_track': return { track, formats, recommendedFormatId: '251' };
          case 'play_track': {
            snapshot = { ...snapshot, status: 'loading', track, activeFormat: formats.find((format) => format.id === args.formatId), durationSeconds: 90 };
            const response = structuredClone(snapshot);
            setTimeout(() => {
              snapshot = { ...snapshot, status: 'playing', decodedSampleRateHz: snapshot.activeFormat.sampleRateHz };
              listener?.({ event: 'playback-state', id: 1, payload: structuredClone(snapshot) });
            }, 50);
            return response;
          }
          case 'set_paused': snapshot.status = args.paused ? 'paused' : 'playing'; break;
          case 'seek_track': snapshot.positionSeconds = args.position; break;
          case 'set_volume': snapshot.volume = args.volume; break;
          case 'stop_playback': snapshot = { ...snapshot, status: 'idle', track: null, activeFormat: null, positionSeconds: 0, durationSeconds: null, decodedSampleRateHz: null }; break;
          default: throw new Error(`Unexpected command: ${command}`);
        }
        return structuredClone(snapshot);
      },
    };
  });

  await page.goto('/');
  await page.getByLabel('Track link').fill('https://music.youtube.com/watch?v=dQw4w9WgXcQ');
  await page.getByRole('button', { name: 'Inspect', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'A fixture song', exact: true })).toHaveCount(1);
  await page.getByLabel('Audio quality').selectOption('140');
  await page.getByRole('button', { name: 'Play track', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Playing', exact: true })).toBeDisabled();
  await expect(page.getByText('44.1 kHz decoded · Audio only')).toBeVisible();
  expect(await page.evaluate(() => window.__vesselCalls.find((call) => call.command === 'play_track').args.formatId)).toBe('140');

  await page.getByRole('heading', { name: 'Start with a song.' }).click();
  await page.keyboard.press('Space');
  await expect(page.getByRole('button', { name: 'Resume playback' })).toBeEnabled();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('slider', { name: 'Playback position' })).toHaveValue('5');

  await page.getByRole('button', { name: 'Stop playback' }).click();
  await expect(page.getByRole('heading', { name: 'Nothing playing yet' })).toBeVisible();
  await expect(page.getByRole('slider', { name: 'Playback position' })).toBeDisabled();
});

test('browser preview explains native playback and remains usable at narrow width', async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 800 });
  await page.goto('/');
  await expect(page.getByText('Browser preview.', { exact: false })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Inspect', exact: true })).toBeDisabled();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
