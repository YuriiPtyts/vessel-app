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
  report.views.some((view) => view.glassStyle === 0 && view.opaque === false),
  'Expected a real Regular-style NSGlassEffectView (macOS 26+), which supports text legibility.',
);
assert.equal(report.web.rootPresent, true, 'The production frontend must have mounted.');
assert.equal(report.web.playerPresent, true);
assert.equal(
  report.web.cleanDarkText,
  true,
  'Low-tint dark labels must retain normal weights, semantic fill, and no stroke or text shadow.',
);
assert(
  Number(report.web.tint) >= 0.5 && Number(report.web.tint) < 1,
  'The glass mode must use the readable 50–99% tint range.',
);
assert.equal(report.web.stageBackground, 'rgba(0, 0, 0, 0)');
console.log(
  `Native material contract passed: clear NSWindow, non-opaque WebView, Regular Liquid Glass, ${Number(report.web.tint) * 100}% tint; ${report.web.homeShelves} live Home shelves.`,
);
