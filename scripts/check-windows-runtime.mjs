import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export async function checkPeX64(path) {
  const bytes = await readFile(path);
  assert(
    bytes.length > 64 && bytes.toString('ascii', 0, 2) === 'MZ',
    'Expected a Windows PE executable.',
  );
  const offset = bytes.readUInt32LE(0x3c);
  assert(
    offset + 26 < bytes.length && bytes.toString('ascii', offset, offset + 4) === 'PE\0\0',
    'Invalid PE header.',
  );
  assert.equal(bytes.readUInt16LE(offset + 4), 0x8664, 'Windows runtime must target x86-64.');
  assert.equal(bytes.readUInt16LE(offset + 24), 0x20b, 'Windows runtime must be PE32+.');
  return bytes;
}

export async function checkWindowsRuntime(root) {
  const engine = await checkPeX64(join(root, 'libmpv-2.dll'));
  assert(
    engine.includes(Buffer.from('-Dgpl=false')),
    'Expected the pinned GPL-disabled libmpv configuration.',
  );
  assert(
    !engine.includes(Buffer.from('libavutil license: GPL version')),
    'GPL FFmpeg is not this build recipe.',
  );
  assert(
    !engine.includes(Buffer.from('--enable-gpl')),
    'GPL-enabled FFmpeg is not this build recipe.',
  );
  assert(
    !engine.includes(Buffer.from('--enable-nonfree')),
    'Nonfree FFmpeg is not this build recipe.',
  );
  await checkPeX64(join(root, 'yt-dlp.exe'));
  await checkPeX64(join(root, 'deno.exe'));
  console.log(
    'Windows x64 runtime PE headers and GPL-disabled libmpv configuration verified; no explicit GPL/nonfree FFmpeg enable flags found.',
  );
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await checkWindowsRuntime(resolve(process.argv[2] ?? 'src-tauri/runtime'));
}
