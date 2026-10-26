import { cp, mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

export async function buildExtractor(dependency, archive, destination, cache) {
  const signature = { source: dependency.sha256, layout: 'arm64-onedir-v1', python: '3.14' };
  const stamp = join(destination, 'vessel-build.json');
  try {
    const previous = JSON.parse(await readFile(stamp, 'utf8'));
    if (
      JSON.stringify(previous) === JSON.stringify(signature) &&
      (await stat(join(destination, 'yt-dlp_macos'))).isFile()
    )
      return;
  } catch (error) {
    if (error.code !== 'ENOENT' && !(error instanceof SyntaxError)) throw error;
  }
  const python = process.env.VESSEL_BUILD_PYTHON ?? 'python3';
  const version = execFileSync(
    python,
    ['-c', 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")'],
    { encoding: 'utf8' },
  ).trim();
  if (version !== '3.14')
    throw new Error(
      'Building the frozen extractor requires developer Python 3.14. Set VESSEL_BUILD_PYTHON to that interpreter. End users do not need Python.',
    );
  const build = join(cache, `extractor-build-${dependency.sha256.slice(0, 12)}`);
  const source = join(build, 'source');
  const environment = {
    ...process.env,
    PYTHONNOUSERSITE: '1',
    PYINSTALLER_CONFIG_DIR: join(build, 'pyinstaller-cache'),
  };
  delete environment.PYTHONPATH;
  await mkdir(source, { recursive: true });
  execFileSync('/usr/bin/tar', ['-xzf', archive, '--strip-components=1', '-C', source]);
  const venv = join(build, 'venv');
  execFileSync(python, ['-m', 'venv', venv], { env: environment, stdio: 'inherit' });
  const interpreter = join(venv, 'bin', 'python');
  console.log('Building a directory-based extractor once, rather than unpacking it on every play…');
  execFileSync(
    interpreter,
    [
      '-m',
      'pip',
      '--isolated',
      'install',
      '--disable-pip-version-check',
      '--only-binary=:all:',
      '--require-hashes',
      '-r',
      'bundle/requirements/pyinstaller.txt',
      '-r',
      'bundle/requirements/default.txt',
    ],
    { cwd: source, env: environment, stdio: 'inherit' },
  );
  execFileSync(interpreter, ['devscripts/make_lazy_extractors.py'], {
    cwd: source,
    env: environment,
    stdio: 'inherit',
  });
  execFileSync(
    interpreter,
    [
      '-m',
      'bundle.pyinstaller',
      '--onedir',
      '--target-architecture',
      'arm64',
      '--distpath',
      join(build, 'dist'),
      '--workpath',
      join(build, 'work'),
    ],
    { cwd: source, env: environment, stdio: 'inherit' },
  );
  const result = join(build, 'dist', 'yt-dlp_macos');
  execFileSync(join(result, 'yt-dlp_macos'), ['--ignore-config', '--no-plugin-dirs', '--version'], {
    env: environment,
    stdio: 'inherit',
  });
  await rm(destination, { recursive: true, force: true });
  await cp(result, destination, { recursive: true, verbatimSymlinks: true });
  await writeFile(stamp, JSON.stringify(signature));
}
