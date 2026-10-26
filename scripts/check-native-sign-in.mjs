import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const path = process.argv[2];
if (!path) throw new Error('Pass the JSON report from VESSEL_SIGN_IN_DIAGNOSTICS.');
const report = JSON.parse(await readFile(path, 'utf8'));
for (const key of [
  'opened',
  'privateStore',
  'googleOrigin',
  'cookiesReadable',
  'windowClosed',
  'accountRestored',
  'localLibraryUnchanged',
]) {
  assert.equal(report[key], true, `Native sign-in contract: ${key}`);
}
console.log(
  'Native sign-in contract passed: private Google webview, Rust cookie access, cancel cleanup, account and local collection preserved. No account credentials were entered.',
);
