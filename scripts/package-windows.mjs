import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { copyFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

assert.equal(process.platform, 'win32', 'Windows installers must be built on Windows.');
const { version } = JSON.parse(await readFile('package.json', 'utf8'));
const bundle = resolve('src-tauri/target/release/bundle/nsis');
const installers = (await readdir(bundle)).filter((file) => file.endsWith('.exe'));
assert.equal(installers.length, 1, 'Expected exactly one native NSIS installer.');
const directory = resolve('.cache/releases', `v${version}`);
await mkdir(directory, { recursive: true });
const name = `Vessel-${version}-windows-x64-setup.exe`;
const output = join(directory, name);
await copyFile(join(bundle, installers[0]), output);
const digest = createHash('sha256');
for await (const chunk of createReadStream(output)) digest.update(chunk);
const sha256 = digest.digest('hex');
await writeFile(join(directory, 'SHA256SUMS-Windows'), `${sha256}  ${name}\n`);
await writeFile(
  join(directory, 'WINDOWS-BUILD.json'),
  `${JSON.stringify({ version, architecture: 'x64', installer: name, sha256, signed: false, windowsNativeBuild: true, audibleOutputTested: false, realAccountTested: false }, null, 2)}\n`,
);
console.log(`Windows installer: ${name}\nSHA-256: ${sha256}`);
