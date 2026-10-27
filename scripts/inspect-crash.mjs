// Print only crash mechanics; leave identities, home paths, and user context out of the output.
import { readFile } from 'node:fs/promises';

const path = process.argv[2];
if (!path) {
  console.error('Usage: node scripts/inspect-crash.mjs /path/to/report.ips');
  process.exit(1);
}
try {
  const source = await readFile(path, 'utf8');
  const report = JSON.parse(source.slice(source.indexOf('\n') + 1));
  const thread = report.threads[report.faultingThread];
  console.log(
    JSON.stringify(
      {
        exception: report.exception?.type,
        signal: report.exception?.signal,
        faultAddress: report.exception?.subtype,
        thread: thread.name ?? thread.queue,
        frames: thread.frames.map((frame) => ({
          image: report.usedImages[frame.imageIndex]?.name,
          symbol: frame.symbol ?? '(unsymbolicated)',
          offset: frame.imageOffset,
        })),
      },
      null,
      2,
    ),
  );
} catch {
  console.error(
    'Could not inspect the report. Check that it exists and is a macOS .ips crash report.',
  );
  process.exitCode = 1;
}
