import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';

test('changes volume while the pointer is still held down', async ({ page }) => {
  await nativeFixture(page);
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/');
  const slider = page.getByRole('slider', { name: 'Volume', exact: true });
  await expect(slider).toBeEnabled();
  const box = await slider.boundingBox();
  await page.mouse.move(box.x + box.width * 0.4, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.85, box.y + box.height / 2, { steps: 8 });
  // This assertion deliberately happens before mouseup: release-only updates cannot pass.
  await expect
    .poll(
      () =>
        page.evaluate(() => {
          const calls = window.__vesselCalls.filter((call) => call.command === 'set_volume');
          return calls.at(-1)?.args.volume ?? -1;
        }),
      { timeout: 2000 },
    )
    .toBeGreaterThan(65);
  await expect(slider).toBeEnabled();
  await page.evaluate(() => window.__vesselEmitSnapshot({ volume: 3 }));
  expect(Number(await slider.inputValue())).toBeGreaterThan(65);
  await page.mouse.up();
});

test('live volume stays responsive and coalesces writes when native replies are slow', async ({
  page,
}) => {
  await nativeFixture(page, { volumeDelay: 90 });
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/');
  const slider = page.getByRole('slider', { name: 'Volume', exact: true });
  await expect(slider).toBeEnabled();
  const box = await slider.boundingBox();
  await page.mouse.move(box.x + box.width * 0.4, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.9, box.y + box.height / 2, { steps: 12 });
  await expect(slider).toBeEnabled();
  await page.evaluate(() => window.__vesselEmitSnapshot({ volume: 3 }));
  expect(Number(await slider.inputValue())).toBeGreaterThan(65);
  await page.mouse.move(box.x + box.width * 0.22, box.y + box.height / 2, { steps: 12 });
  const finalValue = Number(await slider.inputValue());
  await page.mouse.up();
  await expect.poll(() => page.evaluate(() => window.__vesselVolumeInFlight)).toBe(0);
  await expect(slider).toHaveValue(String(finalValue));
  expect(
    await page.evaluate(
      () => window.__vesselCalls.filter((call) => call.command === 'set_volume').at(-1).args.volume,
    ),
  ).toBe(finalValue);
  expect(await page.evaluate(() => window.__vesselMaxVolumeInFlight)).toBe(1);
  expect(
    await page.evaluate(
      () => window.__vesselCalls.filter((call) => call.command === 'set_volume').length,
    ),
  ).toBeLessThan(20);
});

test('Duskfox is the default and appearance choices survive a restart', async ({ page }) => {
  await nativeFixture(page);
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/');
  const frame = page.locator('.app-frame');
  expect(
    await frame.evaluate((element) =>
      getComputedStyle(element).getPropertyValue('--theme-bg').trim(),
    ),
  ).toBe('#232136');
  expect(
    await frame.evaluate((element) =>
      getComputedStyle(element).getPropertyValue('--accent').trim(),
    ),
  ).toBe('#ea9a97');
  await page.getByRole('button', { name: 'Appearance settings' }).click();
  await expect(page.getByText('Duskfox', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Iris accent' }).click();
  await page.getByRole('slider', { name: 'Glass tint' }).evaluate((element) => {
    element.value = '42';
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await page.getByLabel('Song list spacing').selectOption('compact');
  await page.getByRole('switch', { name: /Compact sidebar/ }).click();
  await page.getByRole('switch', { name: /Now playing artwork/ }).click();
  await page.screenshot({ path: test.info().outputPath('customization.png'), fullPage: true });
  await page.reload();
  await expect(frame).toHaveClass(/accent-iris/);
  await expect(frame).toHaveClass(/sidebar-compact/);
  await expect(frame).toHaveClass(/density-compact/);
  await page.getByRole('button', { name: 'Appearance settings' }).click();
  await expect(page.getByRole('slider', { name: 'Glass tint' })).toHaveValue('42');
  await expect(page.getByRole('switch', { name: /Now playing artwork/ })).not.toBeChecked();
  await page.getByRole('button', { name: 'Reset appearance' }).click();
  await expect(frame).toHaveClass(/accent-rose/);
  await expect(page.getByRole('slider', { name: 'Glass tint' })).toHaveValue('56');
  await expect(page.getByRole('switch', { name: /Now playing artwork/ })).toBeChecked();
});

test('track artwork is prominent and falls back if the larger image is unavailable', async ({
  page,
}) => {
  const artworkUrl = 'https://yt3.googleusercontent.com/fixture=w120-h120-l90-rj';
  await nativeFixture(page, { artworkUrl });
  const requests = [];
  await page.route('https://yt3.googleusercontent.com/**', async (route) => {
    requests.push(route.request().url());
    if (route.request().url().includes('w512-h512'))
      return route.fulfill({ status: 404, body: '' });
    await route.fulfill({
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="300" height="300"><rect width="300" height="300" fill="#63577d"/><circle cx="150" cy="150" r="85" fill="#ea9a97"/></svg>',
    });
  });
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  const picture = page.getByRole('img', { name: 'Album artwork for Instant Crush' });
  await expect(picture).toBeVisible();
  await expect.poll(() => picture.evaluate((image) => image.naturalWidth)).toBeGreaterThan(0);
  await expect(picture).toHaveAttribute('src', artworkUrl);
  expect(requests.some((url) => url.includes('w512-h512'))).toBe(true);
  const failedLargeRequests = requests.filter((url) => url.includes('w512-h512')).length;
  for (let volume = 20; volume < 25; volume++) {
    await page.evaluate((volume) => window.__vesselEmitSnapshot({ volume }), volume);
  }
  await expect(picture).toHaveAttribute('src', artworkUrl);
  expect(requests.filter((url) => url.includes('w512-h512')).length).toBe(failedLargeRequests);
  await page.screenshot({ path: test.info().outputPath('track-artwork.png'), fullPage: true });
  await page.getByRole('button', { name: 'Close now playing artwork' }).click();
  await expect(page.getByRole('complementary', { name: 'Now playing artwork' })).not.toBeVisible();
  await page.getByRole('button', { name: 'Show now playing artwork' }).click();
  await expect(picture).toBeVisible();
});

test('searches by artist, plays a result directly, and keeps quality controls in the player', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'A good day for good music.' })).toBeVisible();
  await page.getByRole('searchbox', { name: 'Search music' }).fill('Daft Punk');
  await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
  await expect(page.getByRole('heading', { name: 'Results for “Daft Punk”' })).toBeVisible();
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  expect(
    await page.evaluate(() =>
      window.__vesselCalls.some(
        (call) => call.command === 'play_song' && call.args.videoId === 'khnokW3Mw24',
      ),
    ),
  ).toBe(true);
  expect(
    await page.evaluate(() =>
      window.__vesselCalls.some((call) => call.command === 'inspect_track'),
    ),
  ).toBe(false);

  await page.getByRole('button', { name: 'Audio quality', exact: true }).click();
  await page.getByRole('button', { name: /AAC · ≈128 kbps/ }).click();
  expect(
    await page.evaluate(
      () => window.__vesselCalls.find((call) => call.command === 'play_track').args.formatId,
    ),
  ).toBe('140');
  await page.getByRole('button', { name: 'Close quality options' }).click();
  await page.getByRole('heading', { name: 'Results for “Daft Punk”' }).click();
  await page.keyboard.press('Space');
  await expect(page.getByRole('button', { name: 'Resume playback' })).toBeEnabled();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('slider', { name: 'Playback position' })).toHaveValue('5');
  await page.screenshot({ path: test.info().outputPath('search.png'), fullPage: true });
});

test('saved songs and recent listening work independently of account sign-in', async ({ page }) => {
  await nativeFixture(page);
  await page.goto('/');
  await page.getByRole('searchbox', { name: 'Search music' }).fill('Daft Punk');
  await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
  await page.getByRole('button', { name: 'Save Instant Crush', exact: true }).click();
  await page.getByRole('button', { name: 'Liked songs', exact: false }).click();
  await expect(page.getByRole('heading', { name: 'Liked songs', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback' })).toBeEnabled();
  await page.getByRole('button', { name: 'Recently played', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Pause Instant Crush', exact: true }),
  ).toBeVisible();
});

test('home discovery, appearance, and narrow layout remain usable', async ({ page }) => {
  await nativeFixture(page);
  await page.setViewportSize({ width: 1220, height: 880 });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.screenshot({ path: test.info().outputPath('home.png'), fullPage: true });
  await page.getByRole('button', { name: 'Appearance settings' }).click();
  await page.getByRole('switch', { name: /Reduce transparency/ }).click();
  await expect(page.getByRole('switch', { name: /Reduce transparency/ })).toBeChecked();
  expect(
    await page.locator('.floating-player').evaluate((element) => {
      const style = getComputedStyle(element);
      return [style.backdropFilter, style.webkitBackdropFilter].every(
        (value) => !value || value === 'none',
      );
    }),
  ).toBe(true);
  expect(
    await page.evaluate(
      () =>
        window.__vesselCalls.filter((call) => call.command === 'set_material_mode').at(-1).args
          .reduced,
    ),
  ).toBe(true);
  await page.getByRole('button', { name: 'Close appearance settings' }).click();
  await page.setViewportSize({ width: 420, height: 800 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
  await page.getByRole('button', { name: 'After hours' }).click();
  await expect(page.getByRole('heading', { name: 'Results for “late night jazz”' })).toBeVisible();
  await page.screenshot({ path: test.info().outputPath('narrow.png'), fullPage: true });
});

test('late responses cannot overwrite a newer search', async ({ page }) => {
  await nativeFixture(page);
  await page.goto('/');
  const input = page.getByRole('searchbox', { name: 'Search music' });
  await input.fill('older request');
  await input.press('Enter');
  await input.fill('newer request');
  await input.press('Enter');
  await expect(page.getByRole('heading', { name: 'Results for “newer request”' })).toBeVisible();
  await page.waitForTimeout(500);
  await expect(page.getByRole('heading', { name: 'Results for “newer request”' })).toBeVisible();
});
