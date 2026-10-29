import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const report = JSON.parse(await readFile(process.argv[2], 'utf8'));
assert.equal(report.screenshotsCaptured, false);
assert.equal(report.youtubeAccountUnchanged, true);
assert.equal(report.diagnosticBrowserCleared, true);
if (report.mode === 'interactiveHandoff') {
  assert.equal(report.finished, true);
  assert.equal(report.accountMode, 'connected');
  assert.equal(report.inputValuesRecorded, false);
  assert.equal(report.cookieValuesRecorded, false);
  assert.ok(report.phases.some(({ stage }) => stage === 'sessionActivated'));
} else {
  assert.equal(report.mode, 'credentialFreeRender');
  assert.equal(report.realCredentialsUsed, false);
  assert.equal(report.noUnverifiedConnection, true);
  assert.equal(report.visibleLoginForm, true, 'SoundCloud must render a visible login field.');
  assert.ok(
    report.frames.some(
      (frame) =>
        frame.origin === 'https://secure.soundcloud.com' &&
        frame.ready &&
        frame.visibleLoginFields > 0,
    ),
  );
}
assert.equal(report.passed, true);
console.log(
  `Native SoundCloud ${report.mode} passed with isolated browser data and redacted reporting.`,
);
