import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createReadStream, createWriteStream } from 'node:fs';
import { copyFile, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import { checkWindowsRuntime } from './check-windows-runtime.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));

async function hash(path) {
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(path)) digest.update(chunk);
  return digest.digest('hex');
}

export async function prepareWindows() {
  const manifest = JSON.parse(
    await readFile(join(root, 'native-dependencies.windows.json'), 'utf8'),
  );
  assert.equal(
    `${process.platform}-${process.arch}`,
    manifest.platform,
    'Windows builds require x64 Node.js.',
  );
  const runtime = join(root, 'src-tauri', 'runtime');
  const cache = join(root, '.cache', 'native-windows');
  await mkdir(runtime, { recursive: true });
  await mkdir(cache, { recursive: true });
  for (const dependency of manifest.dependencies) {
    const archive = join(cache, dependency.sha256);
    let valid = false;
    try {
      valid = (await hash(archive)) === dependency.sha256;
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
    }
    if (!valid) {
      console.log(`Downloading ${dependency.name} ${dependency.version} for Windows…`);
      const response = await fetch(dependency.url, { signal: AbortSignal.timeout(180000) });
      assert(response.ok && response.body, `Download failed: ${response.status}`);
      const pending = `${archive}.partial`;
      try {
        await pipeline(Readable.fromWeb(response.body), createWriteStream(pending));
        assert.equal(
          await hash(pending),
          dependency.sha256,
          `Checksum mismatch: ${dependency.name}`,
        );
        await rename(pending, archive);
      } finally {
        await rm(pending, { force: true });
      }
    }
    let source = archive;
    if (dependency.archiveEntry) {
      const extracted = join(cache, `${dependency.name}-extracted`);
      await mkdir(extracted, { recursive: true });
      const tar = join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe');
      execFileSync(tar, ['-xf', archive, '-C', extracted], { stdio: 'inherit' });
      source = join(extracted, dependency.archiveEntry);
    }
    await copyFile(source, join(runtime, dependency.file));
    console.log(`Verified ${dependency.name} ${dependency.version}`);
  }
  const icons = join(root, '.cache', 'windows-icons');
  await checkWindowsRuntime(runtime);
  await mkdir(icons, { recursive: true });
  execFileSync(
    process.execPath,
    [
      join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js'),
      'icon',
      join(root, 'src-tauri', 'icons', '128x128.png'),
      '--output',
      icons,
    ],
    { stdio: 'inherit' },
  );
  await writeFile(join(runtime, 'dependencies.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  await copyFile(join(root, 'docs', 'native-dependencies.md'), join(runtime, 'DEPENDENCIES.md'));
  console.log(`Windows runtime prepared in ${dirname(join(runtime, 'dependencies.json'))}`);
}
