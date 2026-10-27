// Uses an isolated browser/Keychain fixture namespace; never reads Google credentials.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdir, readFile, rm } from 'node:fs/promises';
import { resolve } from 'node:path';

const executable = process.argv[2];
if (process.platform !== 'darwin' || !executable) {
  throw new Error('Pass a native-diagnostics Vessel executable on macOS 14+.');
}
const reports = resolve('.cache/native-remember');
await mkdir(reports, { recursive: true });
const fullProfile = randomUUID().replaceAll('-', '');
const restartProfile = randomUUID().replaceAll('-', '');
for (const stage of ['full', 'seed', 'restore']) {
  const destination = resolve(reports, `${stage}.json`);
  await rm(destination, { force: true });
  await new Promise((success, failure) => {
    const child = spawn(resolve(executable), [], {
      env: {
        ...process.env,
        VESSEL_REMEMBER_STAGE: stage,
        VESSEL_REMEMBER_DIAGNOSTICS: destination,
        VESSEL_TEST_PROFILE_ID: stage === 'full' ? fullProfile : restartProfile,
      },
      stdio: 'inherit',
      detached: true,
    });
    const watchdog = setTimeout(() => {
      try {
        process.kill(-child.pid, 'SIGKILL');
      } catch {
        // The diagnostic process has already exited.
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
      else failure(new Error(`Remembered sign-in ${stage} failed: ${code ?? signal}`));
    });
  });
  const report = JSON.parse(await readFile(destination, 'utf8'));
  assert.equal(report.passed, true);
  assert.equal(report.isolated, true);
  if (stage === 'seed') {
    assert.equal(report.keychainSaved, true);
  }
  assert.equal(report.browserCleared, true);
  if (stage !== 'seed') {
    assert.equal(report.rememberedAfterClose, true);
    assert.equal(report.forgottenAfterReopen, true);
  }
}
console.log(
  'Native remembered sign-in passed: isolated browser profile, Keychain recovery after browser-data loss and process restart, and explicit forgetting. Fixture-only; real Google authentication is not exercised.',
);
