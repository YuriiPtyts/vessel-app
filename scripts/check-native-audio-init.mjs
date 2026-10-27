// Deterministic component regression; no global default-device switching or network.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';

assert.equal(process.platform, 'darwin', 'This CoreAudio regression requires macOS.');
const directory = resolve('.cache/audio-init-regression');
await mkdir(directory, { recursive: true });
const audit = resolve(directory, 'coreaudio-init-audit.dylib');
execFileSync(
  '/usr/bin/clang',
  [
    '-Wall',
    '-Wextra',
    '-Werror',
    '-dynamiclib',
    '-framework',
    'CoreAudio',
    '-framework',
    'AudioToolbox',
    'scripts/native/coreaudio-init-audit.c',
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
assert.equal(build.status, 0, 'Could not build the native component regression.');
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
const test = spawnSync(
  artifact.executable,
  [
    'native_failed_audio_init_releases_device_callbacks',
    '--ignored',
    '--nocapture',
    '--test-threads=1',
  ],
  {
    env: { ...process.env, DYLD_INSERT_LIBRARIES: audit },
    stdio: 'inherit',
    timeout: 30000,
  },
);
assert.equal(test.status, 0, 'CoreAudio initialization failure left invalid device callbacks.');
