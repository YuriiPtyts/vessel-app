import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';
import { renderedTextContrast } from './contrast';

test('long playlists scroll without pushing Appearance or main navigation offscreen', async ({
  page,
}) => {
  await nativeFixture(page, { playlistCount: 80 });
  await page.setViewportSize({ width: 1100, height: 700 });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.evaluate(() => window.__vesselCompleteSignIn());
  const last = page.getByRole('button', { name: 'Saved playlist 80', exact: true });
  await expect(last).toHaveCount(1);
  const appearance = page.getByRole('button', { name: 'Appearance settings', exact: true });
  await expect(appearance).toBeInViewport();
  const scrollArea = await last.evaluate((node) => {
    for (let parent = node.parentElement; parent; parent = parent.parentElement) {
      if (
        parent.scrollHeight > parent.clientHeight &&
        /auto|scroll/.test(getComputedStyle(parent).overflowY)
      ) {
        const box = parent.getBoundingClientRect();
        return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
      }
    }
    return null;
  });
  expect(scrollArea, 'the playlist area must have its own bounded scroll area').not.toBeNull();
  await page.mouse.move(scrollArea.x, scrollArea.y);
  await page.mouse.wheel(0, 100000);
  await expect(last).toBeInViewport();
  await expect(page.getByRole('button', { name: 'Listen now', exact: true })).toBeInViewport();
  await expect(appearance).toBeInViewport();
  await appearance.click();
  await expect(
    page.getByRole('region', { name: 'Appearance settings', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Close appearance settings', exact: true }).click();
  await page.setViewportSize({ width: 420, height: 800 });
  await last.evaluate((node) => {
    const list = node.closest('nav');
    list.scrollTop = list.scrollHeight;
  });
  await expect(last).toBeInViewport();
  await expect(appearance).toBeInViewport();
  await appearance.click();
  await expect(page.getByRole('slider', { name: 'Glass tint', exact: true })).toBeVisible();
});

async function materialSamples(page) {
  const points = await page.evaluate(() =>
    [
      ['sidebar', '.sidebar', 'right', 'middle'],
      ['main', '.main-stage', 'left', 'middle'],
      ['topbar', '.topbar', 'right', 'top'],
      ['player', '.floating-player', 'left', 'bottom'],
      ...(document.querySelector('.appearance-popover')
        ? [['appearance', '.appearance-popover', 'left', 'middle']]
        : []),
    ].map(([name, selector, horizontal, vertical]) => {
      const box = document.querySelector(selector).getBoundingClientRect();
      return {
        name,
        x: horizontal === 'right' ? box.right - 12 : box.left + 12,
        y:
          vertical === 'middle'
            ? box.top + box.height / 2
            : vertical === 'top'
              ? box.top + 12
              : box.bottom - 12,
      };
    }),
  );
  const png = await page.screenshot({ animations: 'disabled' });
  return page.evaluate(
    async ({ png, points }) => {
      const image = new Image();
      image.src = `data:image/png;base64,${png}`;
      await image.decode();
      const canvas = document.createElement('canvas');
      canvas.width = image.naturalWidth;
      canvas.height = image.naturalHeight;
      const context = canvas.getContext('2d');
      context.drawImage(image, 0, 0);
      return points.map(({ name, x, y }) => ({
        name,
        color: [
          ...context.getImageData(
            Math.round((x * canvas.width) / innerWidth),
            Math.round((y * canvas.height) / innerHeight),
            1,
            1,
          ).data,
        ].slice(0, 3),
      }));
    },
    { png: png.toString('base64'), points },
  );
}

test('sidebar, content, top bar and player have the same visible material', async ({ page }) => {
  await nativeFixture(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.setViewportSize({ width: 1400, height: 1000 });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.addStyleTag({
    content:
      'html { background: var(--test-desktop, #fff) !important; } body { background: transparent !important; }',
  });
  await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
  for (const tint of [50, 70, 100]) {
    await page.getByRole('slider', { name: 'Glass tint', exact: true }).evaluate((node, tint) => {
      node.value = String(tint);
      node.dispatchEvent(new Event('input', { bubbles: true }));
    }, tint);
    for (const desktop of ['#fff', '#000']) {
      await page.evaluate(
        (color) => document.documentElement.style.setProperty('--test-desktop', color),
        desktop,
      );
      const samples = await materialSamples(page);
      const baseline = samples.find((sample) => sample.name === 'main').color;
      for (const sample of samples) {
        expect(
          Math.max(...sample.color.map((value, i) => Math.abs(value - baseline[i]))),
          `${tint}/${desktop}: ${JSON.stringify(samples)}`,
        ).toBeLessThanOrEqual(6);
      }
    }
  }
});

test('high-transparency glass uses dark readable text without opaque reading patches', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.setViewportSize({ width: 1400, height: 1000 });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.addStyleTag({
    content: 'html { background: #fff !important; } body { background: transparent !important; }',
  });
  const color = await page
    .locator('.provider-home-heading h1')
    .evaluate((node) => getComputedStyle(node).color);
  expect(color, 'text should darken on the highly transparent light material').toMatch(/^rgb\(/);
  const channels = color
    .match(/[\d.]+/g)
    .slice(0, 3)
    .map(Number);
  expect(Math.max(...channels)).toBeLessThan(80);
  const samples = await renderedTextContrast(page, [
    {
      selector:
        '.sidebar-label, .sidebar nav > button > span, .provider-home-heading p, .provider-card > strong, .provider-card-subtitle',
    },
  ]);
  expect(samples.length).toBeGreaterThan(10);
  expect(samples.filter((sample) => sample.ratio + 0.01 < sample.minimum)).toEqual([]);
});
