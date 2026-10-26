import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const path = process.argv[2];
if (!path) throw new Error('Pass the JSON report produced by a native-diagnostics build.');
const report = JSON.parse(await readFile(path, 'utf8'));
assert.equal(report.windowOpaque, false, 'The real NSWindow must be non-opaque.');
assert.equal(report.windowBackgroundAlpha, 0, 'The native window background must be clear.');
assert(
  report.views.some(
    (view) =>
      view.class.includes('WryWebView') &&
      !view.class.includes('WryWebViewParent') &&
      view.opaque === false,
  ),
);
assert(
  report.views.some((view) => view.glassStyle === 1 && view.opaque === false),
  'Expected a real clear-style NSGlassEffectView (macOS 26+).',
);
assert.equal(report.web.rootPresent, true, 'The production frontend must have mounted.');
assert.equal(report.web.playerPresent, true);
assert(Number(report.web.tint) <= 0.25, 'The web tint must not cover the native backdrop.');
assert.equal(report.web.stageBackground, 'rgba(0, 0, 0, 0)');
console.log(
  `Native material contract passed: clear NSWindow, non-opaque WebView, clear Liquid Glass, ${Number(report.web.tint) * 100}% tint; ${report.web.homeShelves} live Home shelves.`,
);
