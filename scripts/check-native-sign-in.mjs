import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const path = process.argv[2];
if (!path) throw new Error('Pass the JSON report from VESSEL_SIGN_IN_DIAGNOSTICS.');
const report = JSON.parse(await readFile(path, 'utf8'));
for (const key of [
  'opened',
  'googleOrigin',
  'cookiesReadable',
  'windowClosed',
  'accountRestored',
  'localLibraryUnchanged',
  'browserForgotten',
]) {
  assert.equal(report[key], true, `Native sign-in contract: ${key}`);
}
assert.equal(report.persistentStore, report.persistentStoreSupported);
console.log(
  'Native sign-in contract passed: isolated Google webview, expected persistence, Rust cookie access, cancel and forget cleanup, account and local collection preserved. No account credentials were entered.',
);
