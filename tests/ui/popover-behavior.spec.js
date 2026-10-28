import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';

test('account likes are the only liked collection offered', async ({ page }) => {
  await nativeFixture(page, { deviceLikes: true });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect(page.getByRole('button', { name: 'Saved discoveries', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /^Device likes/ })).toHaveCount(0);
  await page.getByRole('button', { name: /^Liked songs/ }).click();
  await expect(page.getByRole('button', { name: 'Play Da Funk', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toHaveCount(
    0,
  );
});

test('popovers keep inside clicks and dismiss on outside clicks without eating the action', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  for (const [triggerName, panelName] of [
    ['Appearance settings', 'Appearance settings'],
    ['Show play queue', 'Play queue'],
    ['Audio quality', 'Audio quality options'],
    ['Open account settings', 'Google account'],
  ]) {
    const trigger = page.getByRole('button', { name: triggerName, exact: true });
    const panel = page.getByRole('region', { name: panelName, exact: true });
    await trigger.click();
    await expect(panel).toBeVisible();
    await panel.locator('.popover-heading').click({ position: { x: 12, y: 12 } });
    await expect(panel).toBeVisible();
    await trigger.click();
    await expect(panel).toHaveCount(0);
    await trigger.click();
    await expect(panel).toBeVisible();
    await page.getByRole('button', { name: /^(Mute|Unmute) audio$/ }).click();
    await expect(panel).toHaveCount(0);
    await trigger.click();
    await expect(panel).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(panel).toHaveCount(0);
  }
  expect(
    await page.evaluate(
      () => window.__vesselCalls.filter((call) => call.command === 'toggle_mute').length,
    ),
  ).toBe(4);
});

test('keyboard opening a different popover closes the previous one', async ({ page }) => {
  await nativeFixture(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Audio quality', exact: true }).click();
  await expect(
    page.getByRole('region', { name: 'Audio quality options', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Open account settings', exact: true }).focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('region', { name: 'Google account', exact: true })).toBeVisible();
  await expect(
    page.getByRole('region', { name: 'Audio quality options', exact: true }),
  ).toHaveCount(0);
});

async function materialPixels(page) {
  const points = await page.evaluate(() => {
    const frame = document.querySelector('.app-frame').getBoundingClientRect();
    const player = document.querySelector('.floating-player').getBoundingClientRect();
    const points = [
      { name: 'window', x: frame.left + 12, y: frame.top + 12 },
      { name: 'player-left', x: player.left + 4, y: player.top + player.height / 2 },
      { name: 'player-right', x: player.right - 4, y: player.top + player.height / 2 },
    ];
    const panel = document.querySelector('.appearance-popover')?.getBoundingClientRect();
    if (panel)
      points.push({ name: 'panel', x: panel.left + 6, y: panel.top + panel.height * 0.65 });
    return points;
  });
  const png = await page.screenshot({ animations: 'allow' });
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

test('material stays painted during popover loading and opening, including both player edges', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.setViewportSize({ width: 1400, height: 1000 });
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  // A small resource-load delay makes the old data-URL mask decoder gap
  // deterministic, rather than relying on the host compositor being slow.
  await page.route('**/__material-delay.svg?**', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 120));
    await route
      .fulfill({
        contentType: 'image/svg+xml',
        body: new URL(route.request().url()).searchParams.get('svg'),
      })
      .catch(() => {});
  });
  await page.addInitScript(() => {
    const setProperty = CSSStyleDeclaration.prototype.setProperty;
    let serial = 0;
    CSSStyleDeclaration.prototype.setProperty = function (name, value, priority) {
      if (name === '--material-mask' && value.includes('data:image/svg+xml,')) {
        const svg = decodeURIComponent(value.match(/data:image\/svg\+xml,([^"']+)/)[1]);
        value = `url("/__material-delay.svg?svg=${encodeURIComponent(svg)}&id=${++serial}")`;
      }
      return setProperty.call(this, name, value, priority);
    };
    const animate = Element.prototype.animate;
    window.__vesselMaterialOpening = null;
    Element.prototype.animate = function (...args) {
      const animation = animate.apply(this, args);
      if (
        this.matches('.appearance-popover, .queue-popover') &&
        Number(animation.effect.getTiming().duration) > 0
      ) {
        animation.effect.updateTiming({ duration: 1200 });
        window.__vesselMaterialOpening = animation;
      }
      return animation;
    };
  });
  await page.goto('/');
  await page.addStyleTag({
    content: 'html { background: #000 !important; } body { background: transparent !important; }',
  });
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  await expect.poll(async () => (await materialPixels(page))[0].color[0]).toBeGreaterThan(100);
  const baseline = await materialPixels(page);
  async function assertPainted(phase) {
    const pixels = await materialPixels(page);
    for (const sample of pixels) {
      const before = baseline.find((entry) => entry.name === sample.name) ?? baseline[0];
      expect(
        Math.max(...sample.color.map((value, channel) => Math.abs(value - before.color[channel]))),
        `${phase}: ${JSON.stringify(pixels)}`,
      ).toBeLessThanOrEqual(8);
    }
  }
  for (const [trigger, close] of [
    ['Appearance settings', 'Close appearance settings'],
    ['Show play queue', 'Close play queue'],
  ]) {
    await page.getByRole('button', { name: trigger, exact: true }).click();
    for (let frame = 0; frame < 4; frame++) {
      await assertPainted(`${trigger}/${frame}`);
    }
    await page.evaluate(() => window.__vesselMaterialOpening.finish());
    await expect
      .poll(() =>
        page
          .locator('.appearance-popover, .queue-popover')
          .evaluate((node) => getComputedStyle(node).transform),
      )
      .toBe('none');
    await assertPainted(`${trigger}/settled`);
    if (trigger === 'Appearance settings') {
      await page.locator('.appearance-scroll').evaluate((node) => {
        node.scrollTop = node.scrollHeight;
      });
      await assertPainted(`${trigger}/scrolled`);
      await page.locator('.appearance-scroll').evaluate((node) => {
        node.scrollTop = 0;
      });
    }
    await page.getByRole('button', { name: close, exact: true }).click();
  }
});
