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

test('late native volume echoes cannot move the slider back after releasing it', async ({
  page,
}) => {
  await nativeFixture(page, { volumeDelay: 90 });
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/');
  const slider = page.getByRole('slider', { name: 'Volume', exact: true });
  const box = await slider.boundingBox();
  await page.mouse.move(box.x + box.width * 0.4, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.82, box.y + box.height / 2, { steps: 8 });
  const final = await slider.inputValue();
  await page.mouse.up();
  await expect.poll(() => page.evaluate(() => window.__vesselVolumeInFlight)).toBe(0);
  await page.evaluate(() => window.__vesselEmitSnapshot({ volume: 3, volumeRevision: 0 }));
  await expect(slider).toHaveValue(final);
});

test('mute and M restore the last audible volume without changing the track', async ({ page }) => {
  await nativeFixture(page);
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  const slider = page.getByRole('slider', { name: 'Volume', exact: true });
  await slider.evaluate((element) => {
    element.value = '73';
    element.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await expect(slider).toHaveValue('73');
  await page.getByRole('button', { name: 'Mute audio', exact: true }).click();
  await expect(slider).toHaveValue('0');
  await page.getByRole('button', { name: 'Unmute audio', exact: true }).click();
  await expect(slider).toHaveValue('73');
  await page.getByRole('heading', { name: 'Listen now.', exact: true }).click();
  await page.keyboard.press('m');
  await expect(slider).toHaveValue('0');
  await page.keyboard.press('m');
  await expect(slider).toHaveValue('73');
  await expect(
    page
      .getByRole('region', { name: 'Music player' })
      .getByRole('heading', { name: 'Instant Crush', exact: true }),
  ).toBeVisible();
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
    element.value = '72';
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
  await expect(page.getByRole('slider', { name: 'Glass tint' })).toHaveValue('72');
  await expect(page.getByRole('switch', { name: /Now playing artwork/ })).not.toBeChecked();
  await page.getByRole('button', { name: 'Reset appearance' }).click();
  await expect(frame).toHaveClass(/accent-rose/);
  await expect(page.getByRole('slider', { name: 'Glass tint' })).toHaveValue('50');
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
  await expect(page.getByRole('heading', { name: 'Listen now.', exact: true })).toBeVisible();
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
  await page.getByRole('button', { name: 'Open Provider Mix' }).click();
  await expect(page.getByRole('heading', { name: 'Provider Mix', exact: true })).toBeVisible();
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

test('Listen now uses provider shelves and collection targets without a fixed artist search', async ({
  page,
}) => {
  await nativeFixture(page, { homeTitle: 'Server-selected recommendations' });
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'Server-selected recommendations' }),
  ).toBeVisible();
  await expect(page.getByText('Guest recommendations', { exact: true })).toBeVisible();
  await expect(page.getByText('A detour with Khruangbin')).toHaveCount(0);
  expect(
    await page.evaluate(() => window.__vesselCalls.some((call) => call.command === 'search_music')),
  ).toBe(false);
  await page.getByRole('button', { name: 'Open Provider Mix' }).click();
  await expect(page.getByRole('heading', { name: 'Provider Mix', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.__vesselCalls.some(
        (call) => call.command === 'browse_collection' && call.args.id === 'VLprovider_mix',
      ),
    ),
  ).toBe(true);
});

test('clear glass upgrades the previous default tint and preserves layout choices', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.addInitScript(() =>
    localStorage.setItem(
      'vessel:appearance:v1',
      JSON.stringify({
        version: 1,
        accent: 'foam',
        glassOpacity: 56,
        density: 'compact',
        sidebarCollapsed: true,
        showNowPlaying: false,
        reducedTransparency: false,
      }),
    ),
  );
  await page.goto('/');
  const frame = page.locator('.app-frame');
  await expect(frame).toHaveClass(/accent-foam/);
  await expect(frame).toHaveClass(/density-compact/);
  await expect(frame).toHaveClass(/sidebar-compact/);
  expect(
    await frame.evaluate((element) =>
      Number(getComputedStyle(element).getPropertyValue('--glass-opacity')),
    ),
  ).toBe(0.5);
  expect(
    await page
      .locator('.main-stage')
      .evaluate((element) => getComputedStyle(element).backgroundColor),
  ).toBe('rgba(0, 0, 0, 0)');
  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('vessel:appearance:v2')).version),
  ).toBe(2);
});

test('glass tint starts at 50%, clamps old low values, and is opaque at 100%', async ({ page }) => {
  await nativeFixture(page);
  await page.addInitScript(() => {
    if (!localStorage.getItem('vessel:appearance:v2'))
      localStorage.setItem(
        'vessel:appearance:v2',
        JSON.stringify({ version: 2, glassOpacity: 8, accent: 'iris', density: 'compact' }),
      );
  });
  await page.goto('/');
  const frame = page.locator('.app-frame');
  await page.getByRole('button', { name: 'Appearance settings' }).click();
  const tint = page.getByRole('slider', { name: 'Glass tint' });
  const setTint = (value) =>
    tint.evaluate((element, value) => {
      element.value = String(value);
      element.dispatchEvent(new Event('input', { bubbles: true }));
      element.dispatchEvent(new Event('change', { bubbles: true }));
    }, value);
  await expect(tint).toHaveAttribute('min', '50');
  await expect(tint).toHaveAttribute('max', '100');
  await expect(tint).toHaveValue('50');
  await expect(frame).toHaveClass(/accent-iris/);
  await expect(frame).toHaveClass(/density-compact/);
  await setTint(100);
  await expect(frame).toHaveClass(/opaque/);
  expect(await frame.evaluate((element) => getComputedStyle(element).backgroundColor)).toBe(
    'rgb(35, 33, 54)',
  );
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__vesselCalls.filter((call) => call.command === 'set_material_mode').at(-1)?.args
            .reduced,
      ),
    )
    .toBe(true);
  await page.reload();
  await expect(frame).toHaveClass(/opaque/);
  await page.getByRole('button', { name: 'Appearance settings' }).click();
  await expect(tint).toHaveValue('100');
  await setTint(50);
  await expect(frame).not.toHaveClass(/opaque/);
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__vesselCalls.filter((call) => call.command === 'set_material_mode').at(-1)?.args
            .reduced,
      ),
    )
    .toBe(false);
});

test('radio queue and transport follow native events without renderer-driven autoplay', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  const player = page.getByRole('region', { name: 'Music player' });
  await page.getByRole('button', { name: 'Show play queue' }).click();
  const queue = page.getByRole('region', { name: 'Play queue', exact: true });
  await expect(queue.getByRole('switch', { name: /Autoplay radio/ })).toBeChecked();
  await expect(
    queue.getByRole('button', { name: 'Play queued Da Funk', exact: true }),
  ).toBeEnabled();
  expect(
    await page.evaluate(
      () => window.__vesselCalls.find((call) => call.command === 'play_song').args.context,
    ),
  ).toEqual([]);
  // This simulates a Rust-owned end-of-track update. The renderer must not issue play_song.
  await page.evaluate(() => window.__vesselAdvanceRadio());
  await expect(player.getByRole('heading', { name: 'Da Funk', exact: true })).toBeVisible();
  await expect(
    queue.getByRole('button', { name: 'Play queued Da Funk', exact: true }),
  ).toHaveAttribute('aria-current', 'true');
  await page.evaluate(() =>
    window.__vesselEmitQueue({ revision: 0, tracks: [], currentIndex: null }),
  );
  await expect(
    queue.getByRole('button', { name: 'Play queued Da Funk', exact: true }),
  ).toHaveAttribute('aria-current', 'true');
  expect(
    await page.evaluate(
      () => window.__vesselCalls.filter((call) => call.command === 'play_song').length,
    ),
  ).toBe(1);
  await queue.getByRole('button', { name: 'Remove Around the World from queue' }).click();
  await expect(
    queue.getByRole('button', { name: 'Play queued Around the World', exact: true }),
  ).toHaveCount(0);
  await queue.getByRole('switch', { name: /Autoplay radio/ }).click();
  await expect(queue.getByRole('switch', { name: /Autoplay radio/ })).not.toBeChecked();
  await queue.getByRole('button', { name: 'Close play queue' }).click();
  await page.getByRole('button', { name: 'Next song', exact: true }).click();
  await expect(
    player.getByRole('heading', { name: 'Harder, Better, Faster, Stronger', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Audio quality', exact: true }).click();
  await page.getByRole('button', { name: /AAC · ≈128 kbps/ }).click();
  expect(
    await page.evaluate(
      () => window.__vesselCalls.find((call) => call.command === 'play_track').args.trackId,
    ),
  ).toBe('LxgnIhjQFuE');
  await page.getByRole('button', { name: 'Close quality options' }).click();
  await page.getByRole('button', { name: 'Previous song', exact: true }).click();
  await expect(player.getByRole('heading', { name: 'Da Funk', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Show play queue' }).click();
  await queue.getByRole('button', { name: 'Clear queue', exact: true }).click();
  await expect(queue.getByText('Start with a song.', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Next song', exact: true })).toBeDisabled();
});

test('Google connection requires a verified account event and sign-out preserves local likes', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.goto('/');
  await page.getByRole('searchbox', { name: 'Search music' }).fill('Daft Punk');
  await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
  await page.getByRole('button', { name: 'Save Instant Crush', exact: true }).click();
  await page.getByRole('button', { name: 'Listen now', exact: true }).click();
  await page.getByRole('button', { name: 'Open account settings' }).click();
  const account = page.getByRole('region', { name: 'Google account', exact: true });
  await account.getByRole('button', { name: 'Connect Google', exact: true }).click();
  await expect(
    account.getByText('Finish signing in in the Google window.', { exact: true }),
  ).toBeVisible();
  await expect(account.getByText('Fixture Listener', { exact: true })).toHaveCount(0);
  await account.getByRole('button', { name: 'Cancel sign-in', exact: true }).click();
  await expect(account.getByRole('button', { name: 'Connect Google', exact: true })).toBeEnabled();
  await account.getByRole('button', { name: 'Connect Google', exact: true }).click();
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect(account.getByText('Fixture Listener', { exact: true })).toBeVisible();
  await expect(page.getByText('Your feed', { exact: true })).toBeVisible();
  await page.evaluate(() => window.__vesselExpireAccount());
  await expect(
    account.getByRole('button', { name: 'Reconnect Google', exact: true }),
  ).toBeEnabled();
  await account.getByRole('button', { name: 'Reconnect Google', exact: true }).click();
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await account.getByRole('button', { name: 'Sign out', exact: true }).click();
  await expect(account.getByText('Listening as a guest', { exact: true })).toBeVisible();
  await expect(page.getByText('Guest recommendations', { exact: true })).toBeVisible();
  await account.getByRole('button', { name: 'Close account settings' }).click();
  await page.getByRole('button', { name: 'Liked songs', exact: false }).click();
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
});

test('a declined Google sign-in keeps guest listening available at narrow widths', async ({
  page,
}) => {
  await nativeFixture(page, { googleRejected: true });
  await page.setViewportSize({ width: 420, height: 800 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Open account settings' }).click();
  const account = page.getByRole('region', { name: 'Google account', exact: true });
  await account.getByRole('button', { name: 'Connect Google', exact: true }).click();
  await expect(account.getByText(/Google declined sign-in in this window/)).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: test.info().outputPath('account-narrow.png'), fullPage: true });
  await account.getByRole('button', { name: 'Close account settings' }).click();
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Show play queue' }).click();
  await expect(page.getByRole('region', { name: 'Play queue', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: test.info().outputPath('queue-narrow.png'), fullPage: true });
});
