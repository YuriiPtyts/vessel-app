import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';

async function ready(page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
}
async function cloud(page) {
  await page.getByRole('switch', { name: 'Switch to SoundCloud', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Play Cloud track 1', exact: true })).toBeVisible();
}

test('provider control is at the exact window centre with search to its right', async ({
  page,
}) => {
  await nativeFixture(page);
  await ready(page);
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback' })).toBeEnabled();
  for (const width of [900, 1220, 1400, 420]) {
    await page.setViewportSize({ width, height: 900 });
    const toggle = page.locator('.provider-switch');
    const box = await toggle.boundingBox();
    expect(Math.abs(box.x + box.width / 2 - width / 2)).toBeLessThan(1);
    const search = await page.getByRole('searchbox', { name: 'Search music' }).boundingBox();
    if (width >= 900) expect(search.x).toBeGreaterThanOrEqual(box.x + box.width + 8);
    else expect(search.y).toBeGreaterThan(box.y + box.height);
    if (width === 1220)
      await page.screenshot({ path: test.info().outputPath('provider-header-desktop.png') });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  }
  await page.screenshot({ path: test.info().outputPath('provider-header-narrow.png') });
});

test('one click slides smoothly between services and reduced motion switches immediately', async ({
  page,
}) => {
  await nativeFixture(page, { providerDelay: 450 });
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await ready(page);
  await page.evaluate(() => {
    const slider = document.querySelector('.provider-switch-slider');
    document.querySelector('.provider-switch').addEventListener(
      'click',
      () => {
        requestAnimationFrame(() => {
          const animation = slider.getAnimations()[0];
          if (animation) {
            animation.pause();
            animation.currentTime = 140;
            window.__vesselProviderAnimation = animation;
          }
        });
      },
      { once: true },
    );
  });
  const start = await page.locator('.provider-switch-slider').boundingBox();
  await page.getByRole('switch', { name: 'Switch to SoundCloud' }).click();
  await expect
    .poll(() => page.evaluate(() => Boolean(window.__vesselProviderAnimation)))
    .toBe(true);
  const middle = await page.locator('.provider-switch-slider').boundingBox();
  expect(middle.x).toBeGreaterThan(start.x + 10);
  expect(middle.x).toBeLessThan(start.x + start.width);
  expect(middle.width).toBeCloseTo(start.width, 1);
  await page.evaluate(() => window.__vesselProviderAnimation.finish());
  await expect(page.getByRole('button', { name: 'Play Cloud track 1', exact: true })).toBeVisible();
  await expect(page.getByRole('switch', { name: 'Switch to YouTube Music' })).toBeChecked();
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.getByRole('switch', { name: 'Switch to YouTube Music' }).click();
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  expect(
    await page
      .locator('.provider-switch-slider')
      .evaluate((node) => getComputedStyle(node).transitionDuration),
  ).toBe('0s');
});

test('both logins coexist and signing out of SoundCloud leaves Google connected', async ({
  page,
}) => {
  await nativeFixture(page);
  await ready(page);
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect(page.getByRole('button', { name: 'Saved discoveries', exact: true })).toBeVisible();
  await cloud(page);
  await page.getByRole('button', { name: 'Open account settings', exact: true }).click();
  await expect(page.getByText('YouTube Music is still connected.', { exact: false })).toBeVisible();
  await page.getByRole('button', { name: 'Connect SoundCloud', exact: true }).click();
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect(
    page
      .getByRole('region', { name: 'SoundCloud account' })
      .getByText('Cloud Listener', { exact: true }),
  ).toBeVisible();
  await expect(page.locator('.provider-connected')).toHaveCount(2);
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Connect SoundCloud', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close account settings', exact: true }).click();
  await page.getByRole('switch', { name: 'Switch to YouTube Music' }).click();
  await expect(page.getByRole('button', { name: 'Saved discoveries', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Open account settings', exact: true }).click();
  await expect(
    page
      .getByRole('region', { name: 'Google account' })
      .getByText('Fixture Listener', { exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.__vesselServiceCalls
        .filter((call) => call.command === 'sign_out')
        .map((call) => call.provider),
    ),
  ).toEqual(['soundcloud']);
});

test('switching pauses music and restores separate queues and listening histories', async ({
  page,
}) => {
  await nativeFixture(page);
  await ready(page);
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback' })).toBeEnabled();
  await cloud(page);
  await page.getByRole('button', { name: 'Play Cloud track 1', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback' })).toBeEnabled();
  await page.getByRole('button', { name: 'Open track on SoundCloud' }).click();
  await expect(page.getByRole('button', { name: 'Show lyrics', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Recently played', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Pause Cloud track 1', exact: true }),
  ).toBeVisible();
  await page.getByRole('switch', { name: 'Switch to YouTube Music' }).click();
  const player = page.getByRole('region', { name: 'Music player' });
  await expect(player.getByRole('heading', { name: 'Instant Crush', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Resume playback' })).toBeEnabled();
  await page.getByRole('button', { name: 'Show play queue' }).click();
  await expect(
    page.getByRole('button', { name: 'Play queued Da Funk', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: /Play queued Cloud track/ })).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        window.__vesselServiceCalls.find((call) => call.command === 'open_track_source').args
          .videoId,
    ),
  ).toBe('sc:1001');
});

test('late YouTube responses cannot overwrite the selected SoundCloud mode', async ({ page }) => {
  await nativeFixture(page);
  await ready(page);
  await page.getByRole('searchbox', { name: 'Search music' }).fill('older request');
  await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
  await cloud(page);
  await page.getByRole('searchbox', { name: 'Search music' }).fill('cloud music');
  await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
  await expect(page.getByRole('button', { name: 'Play Cloud track 1', exact: true })).toBeVisible();
  await page.waitForTimeout(500);
  await page.evaluate(() =>
    window.__vesselEmitTaggedSnapshot('youtubeMusic', {
      track: { id: 'khnokW3Mw24', title: 'Obsolete YouTube song', artist: 'Fixture' },
      status: 'playing',
    }),
  );
  await expect(
    page
      .getByRole('region', { name: 'Music player' })
      .getByRole('heading', { name: 'Obsolete YouTube song' }),
  ).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toHaveCount(
    0,
  );
  await expect(page.getByRole('button', { name: 'Music videos', exact: true })).toHaveCount(0);
  await page.reload();
  await expect(page.locator('.app-frame')).toHaveAttribute('data-provider', 'soundcloud');
  await expect(page.getByRole('button', { name: 'Play Cloud track 1', exact: true })).toBeVisible();
});

test('SoundCloud playlists use their own account and supported visibility', async ({ page }) => {
  await nativeFixture(page);
  await ready(page);
  await cloud(page);
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect(page.getByRole('button', { name: 'Cloud collection', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Create playlist', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Create playlist' });
  await expect(dialog.getByLabel('Visibility').locator('option[value="UNLISTED"]')).toHaveCount(0);
  await dialog.getByLabel('Playlist name').fill('Cloud evenings');
  await dialog.getByRole('button', { name: 'Create playlist', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Cloud evenings', exact: true })).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        window.__vesselServiceCalls.find((call) => call.command === 'create_account_playlist')
          .provider,
    ),
  ).toBe('soundcloud');
});
