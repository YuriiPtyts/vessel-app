import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const report = JSON.parse(await readFile(process.argv[2], 'utf8'));
for (const field of [
  'passed',
  'cookieCapture',
  'browserStoresIsolated',
  'youtubeAccountUnchanged',
  'soundcloudAccountUnchanged',
  'noUnverifiedConnection',
  'windowsClosed',
])
  assert.equal(report[field], true, `Native provider isolation failed: ${field}`);
assert.equal(report.realAccountTested, false);
console.log(
  'Native provider isolation passed: separate cookie stores, SoundCloud cookie capture, unchanged accounts, and clean close. No real credentials used.',
);
