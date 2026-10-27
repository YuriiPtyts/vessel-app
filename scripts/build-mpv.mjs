// Developer-only source build. End users receive the resulting self-contained dylib.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createReadStream, createWriteStream } from 'node:fs';
import { chmod, copyFile, mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

const root = fileURLToPath(new URL('../', import.meta.url));
const patchNames = [
  'mpvkit-library-build.patch',
  'mpv-coreaudio-cleanup.patch',
  'mpv-output-gain.patch',
];

async function hash(path) {
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(path)) digest.update(chunk);
  return digest.digest('hex');
}

async function download(input, cache) {
  const folder = join(cache, 'mpv-inputs', input.sha256);
  await mkdir(folder, { recursive: true });
  const destination = join(folder, input.filename ?? 'archive');
  try {
    if ((await hash(destination)) === input.sha256) return destination;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  const response = await fetch(input.url, { signal: AbortSignal.timeout(180000) });
  if (!response.ok || !response.body)
    throw new Error(`Engine input download failed: ${response.status}`);
  const partial = `${destination}.partial`;
  try {
    await pipeline(Readable.fromWeb(response.body), createWriteStream(partial));
    assert.equal(
      await hash(partial),
      input.sha256,
      'Refusing an engine input with an unexpected hash.',
    );
    await copyFile(partial, destination);
  } finally {
    await rm(partial, { force: true });
  }
  return destination;
}

async function unpack(archive, destination) {
  await mkdir(destination, { recursive: true });
  execFileSync('/usr/bin/tar', ['-xzf', archive, '--strip-components', '1', '-C', destination]);
}

function patch(directory, filename) {
  const env = { ...process.env, GIT_CEILING_DIRECTORIES: dirname(directory) };
  delete env.GIT_DIR;
  delete env.GIT_WORK_TREE;
  delete env.GIT_INDEX_FILE;
  execFileSync('/usr/bin/git', ['apply', filename], { cwd: directory, env, stdio: 'inherit' });
}

export async function buildMpv(dependency, archive, destination, cache) {
  const recipeBytes = await readFile(join(root, 'native/mpv-build.json'));
  const recipe = JSON.parse(recipeBytes);
  assert.equal(dependency.sha256, recipe.mpvkit.sha256);
  const digest = createHash('sha256')
    .update(recipeBytes)
    .update(await readFile(fileURLToPath(import.meta.url)));
  for (const name of patchNames) digest.update(await readFile(join(root, 'native/patches', name)));
  const recipeHash = digest.digest('hex');
  const work = join(cache, `mpv-source-${recipeHash}`);
  const output = join(work, 'libmpv.dylib');
  const stampPath = join(work, 'build.json');
  let stamp;
  try {
    stamp = JSON.parse(await readFile(stampPath, 'utf8'));
    if (stamp.recipe !== recipeHash || stamp.sha256 !== (await hash(output))) stamp = undefined;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  if (!stamp) {
    for (const tool of ['python3', 'swift', 'make', 'pkg-config', 'ninja', 'cmake']) {
      execFileSync('/usr/bin/which', [tool], { stdio: 'ignore' });
    }
    await rm(work, { force: true, recursive: true });
    await mkdir(work, { recursive: true });
    const source = join(work, 'MPVKit');
    await unpack(archive, source);
    patch(source, join(root, 'native/patches/mpvkit-library-build.patch'));
    const dist = join(source, 'dist');
    for (const [name, directory] of [
      ['mpv', 'libmpv-v0.41.0'],
      ['ffmpeg', 'FFmpeg-n8.0.1'],
    ]) {
      const target = join(dist, directory);
      await unpack(await download(recipe[name], cache), target);
      const upstreamPatches = join(
        source,
        'Sources/BuildScripts/patch',
        name === 'mpv' ? 'libmpv' : 'FFmpeg',
      );
      try {
        for (const file of (await readdir(upstreamPatches))
          .filter((file) => file.endsWith('.patch'))
          .sort()) {
          patch(target, join(upstreamPatches, file));
        }
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
      }
    }
    patch(join(dist, 'libmpv-v0.41.0'), join(root, 'native/patches/mpv-coreaudio-cleanup.patch'));
    patch(join(dist, 'libmpv-v0.41.0'), join(root, 'native/patches/mpv-output-gain.patch'));
    for (const input of recipe.archives) {
      const directory = join(dist, `${input.library}-${input.version}`);
      await mkdir(directory, { recursive: true });
      const filename = `${input.library}.zip`;
      await copyFile(await download(input, cache), join(directory, filename));
      execFileSync('/usr/bin/unzip', ['-q', '-o', filename], { cwd: directory });
    }
    const tools = join(work, 'tools');
    execFileSync('python3', ['-m', 'venv', tools], { stdio: 'inherit' });
    execFileSync(
      join(tools, 'bin/python'),
      ['-m', 'pip', 'install', '--no-index', '--no-deps', await download(recipe.meson, cache)],
      { stdio: 'inherit' },
    );
    const env = {
      ...process.env,
      PATH: `${join(tools, 'bin')}:${process.env.PATH}`,
      GIT_CEILING_DIRECTORIES: work,
    };
    console.log(
      'Building the pinned LGPL audio engine with CoreAudio cleanup and live output gain…',
    );
    execFileSync('/usr/bin/make', ['build', 'platform=macos'], {
      cwd: source,
      env,
      stdio: 'inherit',
    });
    const util = join(dist, 'FFmpeg/macos/thin/arm64/lib/libavutil.a');
    assert(
      execFileSync('/usr/bin/strings', [util], {
        encoding: 'utf8',
        maxBuffer: 32 * 1024 * 1024,
      }).includes('libavutil license: LGPL version 3 or later'),
    );
    const archives = [];
    for (const library of await readdir(dist, { withFileTypes: true })) {
      if (!library.isDirectory()) continue;
      const directory = join(dist, library.name, 'macos/thin/arm64/lib');
      try {
        for (const file of await readdir(directory)) {
          if (
            file.endsWith('.a') &&
            !['libmpv.a', 'libssl.a', 'libcrypto.a', 'libsmbclient.a', 'libbluray.a'].includes(file)
          )
            archives.push(join(directory, file));
        }
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
      }
    }
    const sdk = execFileSync('/usr/bin/xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
      encoding: 'utf8',
    }).trim();
    const swift = join(
      dirname(execFileSync('/usr/bin/xcrun', ['--find', 'swiftc'], { encoding: 'utf8' }).trim()),
      '../lib/swift/macosx',
    );
    const exports = join(work, 'exports.txt');
    await writeFile(exports, '_mpv_*\n');
    const frameworks = [
      'AVFoundation',
      'AudioToolbox',
      'CoreAudio',
      'CoreFoundation',
      'CoreMedia',
      'CoreVideo',
      'CoreText',
      'CoreGraphics',
      'Foundation',
      'IOSurface',
      'Metal',
      'QuartzCore',
      'Security',
      'VideoToolbox',
      'AppKit',
      'Cocoa',
      'IOKit',
      'OpenGL',
      'CoreServices',
      'ApplicationServices',
    ];
    execFileSync(
      '/usr/bin/xcrun',
      [
        '--sdk',
        'macosx',
        'clang',
        '-dynamiclib',
        '-target',
        'arm64-apple-macos11.0',
        '-isysroot',
        sdk,
        '-install_name',
        '@rpath/libmpv.dylib',
        '-compatibility_version',
        '2.0',
        '-current_version',
        '2.6',
        `-Wl,-force_load,${join(dist, 'libmpv/macos/thin/arm64/lib/libmpv.a')}`,
        ...archives,
        `-Wl,-exported_symbols_list,${exports}`,
        '-Wl,-dead_strip',
        '-Wl,-no_warn_duplicate_libraries',
        `-L${sdk}/usr/lib/swift`,
        `-L${swift}`,
        ...frameworks.flatMap((name) => ['-framework', name]),
        '-lc++',
        '-lz',
        '-lbz2',
        '-liconv',
        '-lxml2',
        '-lexpat',
        '-lresolv',
        '-o',
        output,
      ],
      { stdio: 'inherit' },
    );
    execFileSync('/usr/bin/strip', ['-x', output]);
    execFileSync('/usr/bin/codesign', ['--force', '--sign', '-', output], { stdio: 'inherit' });
    const dependencies = execFileSync('/usr/bin/otool', ['-L', output], { encoding: 'utf8' })
      .split('\n')
      .slice(2)
      .map((line) => line.trim().split(' (')[0])
      .filter(Boolean);
    assert(
      dependencies.every(
        (path) => path.startsWith('/System/Library/') || path.startsWith('/usr/lib/'),
      ),
      'The engine must link only system dynamic dependencies.',
    );
    stamp = {
      version: dependency.version,
      recipe: recipeHash,
      sha256: await hash(output),
      fix: 'mpv c5d391adba7bd024954d0df1e0405f5749f4d4ca (v0.41.0 backport)',
      outputGain: 'Per-handle native output gain restored before each AO starts streaming',
      inputs: recipe,
    };
    await writeFile(stampPath, JSON.stringify(stamp, null, 2));
  }
  const pending = `${destination}.pending`;
  await copyFile(output, pending);
  await chmod(pending, 0o644);
  await rename(pending, destination);
  await copyFile(stampPath, join(dirname(destination), 'libmpv-build.json'));
}
