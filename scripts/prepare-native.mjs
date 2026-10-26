import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { chmod, copyFile, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { buildExtractor } from './build-extractor.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const manifest = JSON.parse(await readFile(join(root, 'native-dependencies.json'), 'utf8'));

if (`${process.platform}-${process.arch}` !== manifest.platform) {
  throw new Error(`The playback spike currently supports ${manifest.platform} only.`);
}

const runtime = join(root, 'src-tauri', 'runtime');
const cache = join(root, '.cache', 'native');
await mkdir(runtime, { recursive: true });
await mkdir(cache, { recursive: true });

async function hash(path) {
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(path)) digest.update(chunk);
  return digest.digest('hex');
}

for (const dependency of manifest.dependencies) {
  const archive = join(cache, `${dependency.name}-${dependency.sha256}`);
  let valid = false;
  try {
    valid = (await hash(archive)) === dependency.sha256;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }

  if (!valid) {
    console.log(`Downloading ${dependency.name} ${dependency.version}…`);
    const response = await fetch(dependency.url, { signal: AbortSignal.timeout(180_000) });
    if (!response.ok || !response.body) throw new Error(`Download failed: ${response.status}`);
    const temporary = `${archive}.partial`;
    try {
      await pipeline(Readable.fromWeb(response.body), createWriteStream(temporary));
      if ((await hash(temporary)) !== dependency.sha256) {
        throw new Error(`Checksum mismatch for ${dependency.name}; refusing to use the artifact.`);
      }
      await rename(temporary, archive);
    } finally {
      await rm(temporary, { force: true });
    }
  }

  const destination = join(runtime, dependency.file);
  if (dependency.build === 'yt-dlp-onedir') {
    await buildExtractor(dependency, archive, destination, cache);
    const legacy = join(runtime, 'yt-dlp');
    try {
      if (
        (await hash(legacy)) === '7a6fe71d7d680f1fb0ac18d4949873d77db2859e90017c0f44bd94b23be4ee40'
      )
        await rm(legacy);
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
    }
    console.log(`Verified ${dependency.name} ${dependency.version} directory runtime`);
    continue;
  } else if (dependency.zipEntry) {
    const extracted = join(cache, `${dependency.name}-extracted`);
    await mkdir(extracted, { recursive: true });
    execFileSync('/usr/bin/ditto', ['-x', '-k', archive, extracted]);
    await copyFile(join(extracted, dependency.zipEntry), destination);
  } else {
    await copyFile(archive, destination);
  }
  await chmod(destination, dependency.executable ? 0o755 : 0o644);
  if (dependency.name === 'libmpv') {
    const linked = execFileSync('/usr/bin/otool', ['-arch', 'arm64', '-L', destination], {
      encoding: 'utf8',
    })
      .split('\n')
      .slice(2)
      .map((line) => line.trim().split(' (')[0])
      .filter(Boolean);
    if (
      linked.some((path) => !path.startsWith('/System/Library/') && !path.startsWith('/usr/lib/'))
    ) {
      throw new Error(
        'libmpv references non-system libraries; refusing a non-self-contained runtime.',
      );
    }
  }
  console.log(`Verified ${dependency.name} ${dependency.version}`);
}

await writeFile(join(runtime, 'dependencies.json'), JSON.stringify(manifest, null, 2));
await copyFile(join(root, 'docs', 'native-dependencies.md'), join(runtime, 'DEPENDENCIES.md'));
console.log(`Native runtime prepared in ${dirname(join(runtime, 'dependencies.json'))}`);
