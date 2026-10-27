// Measure rendered gain changes, not property acknowledgement. The audit image
// silences every observed CoreAudio block before it reaches the physical output.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';

assert.equal(process.platform, 'darwin', 'The native render probe requires macOS.');
const directory = resolve('.cache/volume-render-regression');
await mkdir(directory, { recursive: true });
const audit = resolve(directory, 'volume-render-audit.dylib');
execFileSync(
  '/usr/bin/clang',
  [
    '-Wall',
    '-Wextra',
    '-Werror',
    '-dynamiclib',
    '-framework',
    'AudioToolbox',
    '-framework',
    'AVFoundation',
    '-framework',
    'CoreMedia',
    '-framework',
    'Foundation',
    'scripts/native/volume-render-audit.c',
    'scripts/native/volume-render-avfoundation.m',
    '-o',
    audit,
  ],
  { stdio: 'inherit' },
);
const build = spawnSync(
  'cargo',
  [
    'test',
    '--manifest-path',
    'src-tauri/Cargo.toml',
    '--locked',
    '--lib',
    '--no-run',
    '--message-format=json',
  ],
  {
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'inherit'],
    timeout: 180000,
  },
);
assert.equal(build.status, 0, 'Could not build the rendered-volume regression.');
const artifact = build.stdout
  .split('\n')
  .filter(Boolean)
  .map((line) => JSON.parse(line))
  .find(
    (message) =>
      message.reason === 'compiler-artifact' &&
      message.target.name === 'vessel_app' &&
      message.profile.test &&
      message.executable,
  );
assert(artifact?.executable);
for (const backend of process.env.VESSEL_VOLUME_PROBE_BACKEND
  ? [process.env.VESSEL_VOLUME_PROBE_BACKEND]
  : ['coreaudio', 'avfoundation']) {
  // Each fixture owns one player's renderer timeline, isolated from earlier tests.
  for (const test of [
    'native_output_gain_is_restored_before_recreated_output_starts',
    'native_output_gain_follows_volume_without_queued_audio_delay',
  ]) {
    const result = spawnSync(
      artifact.executable,
      [test, '--ignored', '--nocapture', '--test-threads=1'],
      {
        env: { ...process.env, VESSEL_VOLUME_PROBE_BACKEND: backend, DYLD_INSERT_LIBRARIES: audit },
        stdio: 'inherit',
        timeout: 30000,
      },
    );
    assert.equal(
      result.status,
      0,
      `${backend} ${test} failed (${result.signal ?? result.error?.code ?? result.status}).`,
    );
  }
}
