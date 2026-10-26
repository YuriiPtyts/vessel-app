import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';

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
