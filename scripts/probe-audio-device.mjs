// Bound the opt-in native hotplug probe: a broken native AO may hang in mpv teardown.
import { spawn } from 'node:child_process';

if (process.platform !== 'darwin') {
  console.error('This probe requires macOS, Xcode/Swift, and the prepared native runtime.');
  process.exit(1);
}
const timeout = Number(process.env.VESSEL_AUDIO_PROBE_TIMEOUT_MS ?? 45000);
if (!Number.isFinite(timeout) || timeout < 1000 || timeout > 600000) {
  console.error('VESSEL_AUDIO_PROBE_TIMEOUT_MS must be between 1000 and 600000.');
  process.exit(1);
}
const child = spawn(
  'cargo',
  [
    'test',
    '--manifest-path',
    'src-tauri/Cargo.toml',
    '--locked',
    '--lib',
    'native_audio_survives_device_publication_and_removal',
    '--',
    '--ignored',
    '--nocapture',
    '--test-threads=1',
  ],
  {
    env: { ...process.env, VESSEL_AUDIO_STRESS: '1' },
    stdio: 'inherit',
    detached: true,
  },
);
let timedOut = false;
let interrupted = false;
let forceKill;
function terminate(signal) {
  if (!child.pid) return;
  try {
    process.kill(-child.pid, signal);
  } catch {
    // The probe's process group has already exited.
  }
}
const timer = setTimeout(() => {
  timedOut = true;
  console.error(`Native device probe timed out after ${timeout}ms; terminating its process group.`);
  terminate('SIGTERM');
  forceKill = setTimeout(() => terminate('SIGKILL'), 1000);
}, timeout);
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => {
    interrupted = true;
    clearTimeout(timer);
    terminate(signal);
    forceKill ??= setTimeout(() => terminate('SIGKILL'), 1000);
  });
}
child.on('error', () => {
  clearTimeout(timer);
  console.error('Could not start cargo for the native device probe.');
  process.exitCode = 1;
});
child.on('close', (code) => {
  clearTimeout(timer);
  clearTimeout(forceKill);
  process.exitCode = timedOut ? 1 : interrupted ? 130 : (code ?? 1);
});
