import { execFileSync } from 'node:child_process';
import { access, lstat, mkdir, mkdtemp, open, readdir, readlink, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const machOMagic = new Set([
  'feedface',
  'feedfacf',
  'cefaedfe',
  'cffaedfe',
  'cafebabe',
  'bebafeca',
  'cafebabf',
  'bfbafeca',
]);

export function run(command, args) {
  try {
    return execFileSync(command, args, {
      encoding: 'utf8',
      timeout: 900000,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (error) {
    throw new Error(
      `${basename(command)} failed: ${error.stderr?.trim() || error.stdout?.trim() || error.message}`,
    );
  }
}

// Resources contain executable code too, including a versioned Python framework.
// codesign --deep alone does not discover all code in these nonstandard locations.
export async function signingTargets(app) {
  const targets = [];
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(path);
        if (entry.name.endsWith('.framework') || entry.name.endsWith('.app')) targets.push(path);
      } else if (entry.isFile()) {
        const file = await open(path, 'r');
        try {
          const buffer = Buffer.alloc(4);
          const { bytesRead } = await file.read(buffer, 0, 4, 0);
          if (bytesRead === 4 && machOMagic.has(buffer.toString('hex'))) targets.push(path);
        } finally {
          await file.close();
        }
      }
      // Do not follow framework aliases or other symlinks; their real targets are visited.
    }
  }
  await visit(app);
  targets.push(app);
  return targets.sort((a, b) => b.split(sep).length - a.split(sep).length);
}

export async function verifyApp(app, { distribution = false } = {}) {
  run('/usr/bin/codesign', ['--verify', '--deep', '--strict', app]);
  await access(join(app, 'Contents', '_CodeSignature', 'CodeResources'));
  const targets = await signingTargets(app);
  for (const target of targets) run('/usr/bin/codesign', ['--verify', '--strict', target]);
  if (distribution) {
    run('/usr/bin/xcrun', ['stapler', 'validate', app]);
    run('/usr/sbin/spctl', ['--assess', '--type', 'execute', '--verbose=4', app]);
  }
  return targets.length;
}

export async function verifyArchive(archive, options) {
  const temporary = await mkdtemp(join(tmpdir(), 'vessel-archive-check-'));
  try {
    run('/usr/bin/ditto', ['-x', '-k', archive, temporary]);
    return await verifyApp(join(temporary, 'Vessel.app'), options);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

export async function verifyDiskImage(image, options) {
  run('/usr/bin/hdiutil', ['verify', image]);
  const temporary = await mkdtemp(join(tmpdir(), 'vessel-dmg-check-'));
  const mount = join(temporary, 'volume');
  await mkdir(mount);
  let mounted = false;
  try {
    run('/usr/bin/hdiutil', ['attach', '-readonly', '-nobrowse', '-mountpoint', mount, image]);
    mounted = true;
    const applications = join(mount, 'Applications');
    if (
      !(await lstat(applications)).isSymbolicLink() ||
      (await readlink(applications)) !== '/Applications'
    ) {
      throw new Error('The DMG must contain an Applications shortcut targeting /Applications.');
    }
    const count = await verifyApp(join(mount, 'Vessel.app'), options);
    if (options?.distribution) {
      run('/usr/bin/xcrun', ['stapler', 'validate', image]);
      run('/usr/sbin/spctl', [
        '--assess',
        '--type',
        'open',
        '--context',
        'context:primary-signature',
        '--verbose=4',
        image,
      ]);
    }
    return count;
  } finally {
    // If detaching fails, keep the mount directory rather than deleting through a live mount.
    if (mounted) run('/usr/bin/hdiutil', ['detach', mount]);
    await rm(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.platform !== 'darwin') throw new Error('Package verification requires macOS.');
    const [input, ...flags] = process.argv.slice(2);
    if (!input || flags.some((flag) => flag !== '--distribution')) {
      throw new Error(
        'Usage: node scripts/check-macos-package.mjs <app-zip-or-dmg> [--distribution]',
      );
    }
    const options = { distribution: flags.includes('--distribution') };
    const count = input.endsWith('.zip')
      ? await verifyArchive(resolve(input), options)
      : input.endsWith('.dmg')
        ? await verifyDiskImage(resolve(input), options)
        : await verifyApp(resolve(input), options);
    console.log(
      `Verified bundle resource seal and ${count} code signatures${input.endsWith('.dmg') ? ', DMG integrity, and the Applications shortcut' : ''}${options.distribution ? ', stapled notarization, and Gatekeeper acceptance' : ' (signature integrity only; not Gatekeeper approval)'}.`,
    );
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
