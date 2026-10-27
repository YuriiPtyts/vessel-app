// Native MediaPlayer registration/state and handler-routing check. It does not
// synthesize physical keyboard events or touch the user's saved collection.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdir, readFile, rm } from 'node:fs/promises';
import { resolve } from 'node:path';

const executable = process.argv[2];
assert(
  process.platform === 'darwin' && executable,
  'Pass a native-diagnostics executable on macOS.',
);
const directory = resolve('.cache/native-media');
await mkdir(directory, { recursive: true });
const destination = resolve(directory, 'report.json');
await rm(destination, { force: true });
await new Promise((success, failure) => {
  const child = spawn(resolve(executable), [], {
    env: { ...process.env, VESSEL_MEDIA_DIAGNOSTICS: destination },
    stdio: 'inherit',
    detached: true,
  });
  const watchdog = setTimeout(() => {
    try {
      process.kill(-child.pid, 'SIGKILL');
    } catch {
      /* Already exited. */
    }
  }, 30000);
  let quitting = false;
  const ready = setInterval(async () => {
    if (quitting) return;
    try {
      JSON.parse(await readFile(destination, 'utf8'));
    } catch {
      return;
    }
    quitting = true;
    const quit = spawnSync(
      '/usr/bin/swift',
      [
        '-e',
        `import AppKit; import Darwin; guard let app = NSRunningApplication(processIdentifier: ${child.pid}), app.terminate() else { exit(1) }`,
      ],
      { timeout: 15000, stdio: 'inherit' },
    );
    if (quit.status !== 0) child.kill('SIGKILL');
  }, 100);
  child.on('error', (error) => {
    clearTimeout(watchdog);
    clearInterval(ready);
    failure(error);
  });
  child.on('close', (code, signal) => {
    clearTimeout(watchdog);
    clearInterval(ready);
    if (code === 0) success();
    else failure(new Error(`Native media check failed: ${code ?? signal}`));
  });
});
const report = JSON.parse(await readFile(destination, 'utf8'));
assert.equal(report.passed, true);
assert.equal(report.registeredCommands, 5);
assert.equal(report.nativeNowPlaying, true);
assert.equal(report.physicalKeyboardTested, false);
console.log(
  'Native media commands passed: previous/next, play/pause/toggle handlers, Now Playing state, and inactive-command cleanup. Physical keyboard routing still needs owner confirmation.',
);
