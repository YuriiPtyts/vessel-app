import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { copyFile, mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { run, signingTargets, verifyApp, verifyArchive } from './check-macos-package.mjs';

const flags = process.argv.slice(2);
const distribution = flags.includes('--distribution');
if (process.platform !== 'darwin' || flags.some((flag) => flag !== '--distribution')) {
  throw new Error('Usage on macOS: node scripts/package-macos.mjs [--distribution]');
}

const identity = distribution ? process.env.VESSEL_SIGNING_IDENTITY : '-';
const profile = process.env.VESSEL_NOTARY_PROFILE;
if (distribution && (!identity || identity === '-' || !profile)) {
  throw new Error(
    'Warning-free distribution requires a Developer ID Application certificate/private key in Keychain, VESSEL_SIGNING_IDENTITY, and VESSEL_NOTARY_PROFILE (a notarytool Keychain profile). Ad-hoc signing is not sufficient.',
  );
}

const root = fileURLToPath(new URL('../', import.meta.url));
const config = JSON.parse(await readFile(join(root, 'src-tauri', 'tauri.conf.json'), 'utf8'));
const npm = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
if (npm.version !== config.version) throw new Error('npm and Tauri versions must match.');
const name = `Vessel-${config.version}-macos-arm64.zip`;
const destination = join(root, '.cache', 'releases', `v${config.version}`);
// Stage outside synced workspaces: File Provider may reattach FinderInfo while signing.
const temporary = await mkdtemp(join(tmpdir(), 'vessel-macos-package-'));

try {
  const app = join(temporary, 'Vessel.app');
  const archive = join(temporary, name);
  run('/usr/bin/ditto', [
    '--norsrc',
    '--noextattr',
    join(root, 'src-tauri', 'target', 'release', 'bundle', 'macos', 'Vessel.app'),
    app,
  ]);
  // Tauri's resource copy flattens aliases in the frozen Python framework.
  // Restore the prepared layout before signing, preserving relative symlinks.
  const runtime = join(app, 'Contents', 'Resources', 'runtime');
  await rm(runtime, { recursive: true, force: true });
  run('/usr/bin/ditto', ['--norsrc', '--noextattr', join(root, 'src-tauri', 'runtime'), runtime]);

  const targets = await signingTargets(app);
  for (const target of targets) {
    run('/usr/bin/codesign', [
      '--force',
      '--sign',
      identity,
      '--preserve-metadata=entitlements',
      ...(distribution ? ['--options', 'runtime', '--timestamp'] : ['--timestamp=none']),
      ...(target === app ? ['--identifier', config.identifier] : []),
      target,
    ]);
  }
  await verifyApp(app);

  async function archiveApp() {
    await rm(archive, { force: true });
    run('/usr/bin/ditto', ['-c', '-k', '--norsrc', '--noextattr', '--keepParent', app, archive]);
  }
  await archiveApp();
  if (distribution) {
    const submission = JSON.parse(
      run('/usr/bin/xcrun', [
        'notarytool',
        'submit',
        archive,
        '--keychain-profile',
        profile,
        '--wait',
        '--output-format',
        'json',
      ]),
    );
    if (submission.status !== 'Accepted') {
      throw new Error(`Apple notarization was not accepted (${submission.status}).`);
    }
    run('/usr/bin/xcrun', ['stapler', 'staple', app]);
    await verifyApp(app, { distribution: true });
    await archiveApp();
  }

  // The deliverable must still verify after a normal unzip, not just before archiving.
  const verifiedTargets = await verifyArchive(archive, { distribution });
  const gatekeeper = spawnSync(
    '/usr/sbin/spctl',
    ['--assess', '--type', 'execute', '--verbose=4', app],
    { encoding: 'utf8', timeout: 120000 },
  );
  const gatekeeperAccepted = gatekeeper.status === 0;
  if (distribution && !gatekeeperAccepted) throw new Error('Gatekeeper rejected the final app.');
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(archive)) digest.update(chunk);
  const sha256 = digest.digest('hex');

  await mkdir(destination, { recursive: true });
  const output = join(destination, name);
  await copyFile(archive, `${output}.partial`);
  await rename(`${output}.partial`, output);
  await writeFile(join(destination, 'SHA256SUMS'), `${sha256}  ${name}\n`);
  await writeFile(
    join(destination, 'SIGNING.json'),
    `${JSON.stringify(
      {
        version: config.version,
        archive: name,
        sha256,
        signing: distribution ? 'developer-id' : 'ad-hoc',
        notarized: distribution,
        gatekeeperAccepted,
        verifiedCodeSignatures: verifiedTargets,
        verifiedAfterUnzip: true,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Packaged ${name}; ${verifiedTargets} signatures and the unzip round-trip verified.`);
  console.log(`SHA-256: ${sha256}`);
  console.log(
    distribution
      ? 'Developer ID signed, notarized, stapled, and Gatekeeper accepted.'
      : `Ad-hoc development build. Gatekeeper accepted: ${gatekeeperAccepted}. Warning-free distribution requires npm run release:macos with Apple signing credentials.`,
  );
} finally {
  await rm(temporary, { recursive: true, force: true });
}
