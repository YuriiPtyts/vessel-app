import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';
import { renderedTextContrast } from './contrast';

async function backgroundAlpha(panel) {
  return panel.evaluate((node) => {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 1;
    const context = canvas.getContext('2d');
    context.fillStyle = getComputedStyle(node, '::before').backgroundColor;
    context.fillRect(0, 0, 1, 1);
    return context.getImageData(0, 0, 1, 1).data[3] / 255;
  });
}

test('Appearance and Queue share one window material with readable text and opaque mode', async ({
  page,
}, testInfo) => {
  await nativeFixture(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.setViewportSize({ width: 1400, height: 1000 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  await page.addStyleTag({
    content: 'html { background: #fff !important; } body { background: transparent !important; }',
  });
  const appearance = page.getByRole('region', { name: 'Appearance settings', exact: true });
  const queue = page.getByRole('region', { name: 'Play queue', exact: true });
  const frame = page.locator('.app-frame');
  for (const [tint, reduced] of [
    [50, false],
    [75, false],
    [100, false],
    [50, true],
  ]) {
    await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
    await appearance
      .getByRole('slider', { name: 'Glass tint', exact: true })
      .evaluate((node, tint) => {
        node.value = String(tint);
        node.dispatchEvent(new Event('input', { bubbles: true }));
      }, tint);
    await expect.poll(() => backgroundAlpha(frame)).toBeCloseTo(tint / 100, 2);
    await expect.poll(() => backgroundAlpha(appearance)).toBeCloseTo(tint / 100, 2);
    await appearance.getByRole('slider', { name: 'Glass tint', exact: true }).evaluate((node) => {
      node.dispatchEvent(new Event('change', { bubbles: true }));
    });
    if (reduced) await appearance.getByRole('switch', { name: /Reduce transparency/ }).click();
    const alpha = reduced ? 1 : tint / 100;
    await expect.poll(() => backgroundAlpha(frame)).toBeCloseTo(alpha, 2);
    await expect.poll(() => backgroundAlpha(appearance)).toBeCloseTo(alpha, 2);
    await page.screenshot({ path: testInfo.outputPath(`appearance-${tint}-${reduced}.png`) });
    const labels = await renderedTextContrast(page, [
      {
        selector:
          '.appearance-popover .popover-heading, .appearance-section h3, .preference-label, .appearance-section > p, .appearance-option strong, .appearance-option small, .reset-appearance',
      },
    ]);
    expect(labels.length).toBeGreaterThan(8);
    expect(labels.filter((sample) => sample.ratio + 0.01 < sample.minimum)).toEqual([]);
    await page.getByRole('button', { name: 'Close appearance settings', exact: true }).click();
    await page.getByRole('button', { name: 'Show play queue', exact: true }).click();
    await expect.poll(() => backgroundAlpha(frame)).toBeCloseTo(alpha, 2);
    await expect.poll(() => backgroundAlpha(queue)).toBeCloseTo(alpha, 2);
    const tracks = await renderedTextContrast(page, [
      {
        selector:
          '.queue-popover .popover-heading, .queue-popover .radio-option strong, .queue-popover .radio-option small, .queue-copy strong, .queue-copy small, .queue-footer, .queue-seed',
        includeDisabled: true,
      },
    ]);
    expect(tracks.length).toBeGreaterThan(5);
    expect(tracks.filter((sample) => sample.ratio + 0.01 < sample.minimum)).toEqual([]);
    await page.screenshot({ path: testInfo.outputPath(`queue-${tint}-${reduced}.png`) });
    const filter = await queue.evaluate((node) => getComputedStyle(node, '::before').backdropFilter);
    if (alpha === 1) {
      expect(filter).toBe('none');
    } else {
      expect(filter).toContain('blur(');
    }
    await page.getByRole('button', { name: 'Close play queue', exact: true }).click();
  }
});
