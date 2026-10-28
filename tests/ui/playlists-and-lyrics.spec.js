import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';

async function start(page, connected = true) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  if (connected) {
    await page.evaluate(() => window.__vesselCompleteSignIn());
    await expect(
      page.getByRole('button', { name: 'Saved discoveries', exact: true }),
    ).toBeVisible();
  }
}

test('create a private account playlist, open it, and add a searched song', async ({ page }) => {
  await nativeFixture(page, { readOnlyPlaylist: true });
  await start(page);
  await page.getByRole('button', { name: 'Create playlist', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Create playlist', exact: true });
  await expect(dialog.getByLabel('Playlist name')).toBeFocused();
  await dialog.getByLabel('Playlist name').fill('Late evening');
  await expect(dialog.getByLabel('Visibility')).toHaveValue('PRIVATE');
  await dialog.getByRole('button', { name: 'Create playlist', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Late evening', exact: true })).toBeVisible();
  await expect(page.getByText('Your playlist is ready for music.')).toBeVisible();
  await page.getByRole('button', { name: 'Find songs', exact: true }).click();
  await page.getByRole('searchbox', { name: 'Search music' }).fill('Daft Punk');
  await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
  await page.getByRole('button', { name: 'Add Da Funk to playlist', exact: true }).click();
  const picker = page.getByRole('dialog', { name: 'Add to playlist', exact: true });
  await expect(picker.getByRole('button', { name: 'Someone else’s playlist' })).toHaveCount(0);
  await picker.getByRole('button', { name: 'Late evening', exact: true }).click();
  await expect(picker).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Play Da Funk', exact: true })).toBeVisible();
  const writes = await page.evaluate(() =>
    window.__vesselCalls.filter((call) =>
      ['create_account_playlist', 'add_to_account_playlist'].includes(call.command),
    ),
  );
  expect(writes.map((call) => call.args)).toEqual([
    {
      title: 'Late evening',
      description: '',
      privacy: 'PRIVATE',
      videoId: null,
      accountRevision: 1,
    },
    { id: 'VLPLcreated1', videoId: 'd7woYjGWs6k', accountRevision: 1 },
  ]);
});

test('player can create a playlist containing its song and narrow dialogs remain usable', async ({
  page,
}) => {
  await nativeFixture(page);
  await start(page);
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  await page.setViewportSize({ width: 420, height: 800 });
  await page.getByRole('button', { name: 'Add current song to playlist' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add to playlist' });
  await dialog.getByRole('button', { name: 'New playlist', exact: true }).click();
  await dialog.getByLabel('Playlist name').fill('Night drive');
  await dialog.getByLabel('Visibility').selectOption('UNLISTED');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: test.info().outputPath('playlist-narrow.png') });
  await dialog.getByRole('button', { name: 'Create and add song' }).click();
  await expect(page.getByRole('heading', { name: 'Night drive', exact: true })).toBeVisible();
  expect(
    await page.evaluate(
      () => window.__vesselCalls.find((call) => call.command === 'create_account_playlist').args,
    ),
  ).toMatchObject({ videoId: 'khnokW3Mw24', privacy: 'UNLISTED' });
});

test('unconfirmed playlist writes show an error without closing or automatically retrying', async ({
  page,
}) => {
  await nativeFixture(page, { playlistFailure: true, playlistDelay: 150 });
  await start(page);
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Add current song to playlist' })).toBeEnabled();
  await page.getByRole('button', { name: 'Add current song to playlist' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add to playlist' });
  await dialog.getByRole('button', { name: 'Saved discoveries', exact: true }).click();
  await expect(dialog.getByRole('status')).toContainText('Waiting for YouTube Music');
  await expect(dialog.getByRole('alert')).toContainText('did not confirm');
  await expect(dialog).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        window.__vesselCalls.filter((call) => call.command === 'add_to_account_playlist').length,
    ),
  ).toBe(1);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Add current song to playlist' })).toBeFocused();
});

test('guests see account connection and a late creation cannot reopen a signed-out library', async ({
  page,
}) => {
  await nativeFixture(page, { playlistDelay: 400 });
  await start(page, false);
  await page.getByRole('button', { name: 'Create playlist', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Connect to make playlists.' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect(page.getByRole('button', { name: 'Saved discoveries', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Create playlist', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Create playlist' });
  await dialog.getByLabel('Playlist name').fill('Obsolete');
  await dialog.getByRole('button', { name: 'Create playlist', exact: true }).click();
  await page.evaluate(() => window.__vesselExpireAccount());
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => window.__vesselPlaylistReplies)).toBe(1);
  await expect(page.getByRole('heading', { name: 'Obsolete', exact: true })).toHaveCount(0);
  await expect(page.locator('.sidebar-playlists button', { hasText: 'Obsolete' })).toHaveCount(0);
});

test('lyrics follow playback, ignore late old lyrics, scroll and restore focus on close', async ({
  page,
}) => {
  await nativeFixture(page, { lyricsDelay: 450 });
  await start(page, false);
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  const trigger = page.getByRole('button', { name: 'Show lyrics', exact: true });
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: 'Lyrics', exact: true });
  await expect(dialog.getByRole('status')).toContainText('Finding lyrics');
  await page.evaluate(() => window.__vesselAdvanceRadio());
  await expect(dialog.locator('.lyrics-copy')).toContainText('Next fixture song');
  await page.waitForTimeout(500);
  await expect(dialog.locator('.lyrics-copy')).not.toContainText('First fixture song');
  await expect(dialog).toContainText('Da Funk');
  expect(
    await dialog
      .locator('.feature-dialog-scroll')
      .evaluate((node) => node.scrollHeight > node.clientHeight),
  ).toBe(true);
  await page.setViewportSize({ width: 420, height: 800 });
  await page.screenshot({ path: test.info().outputPath('lyrics-narrow.png') });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await trigger.click();
  await expect(dialog).toBeVisible();
  await page.evaluate(() => window.__vesselEmitSnapshot({ track: null, status: 'idle' }));
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  await page.keyboard.press('Control+k');
  await expect(page.getByRole('searchbox', { name: 'Search music' })).toBeFocused();
});

for (const state of ['missing', 'failed'])
  test(`lyrics ${state} state is explicit and failures can retry`, async ({ page }) => {
    await nativeFixture(page, { noLyrics: state === 'missing', lyricsFailure: state === 'failed' });
    await start(page, false);
    await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
    await page.getByRole('button', { name: 'Show lyrics', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Lyrics', exact: true });
    if (state === 'missing')
      await expect(
        dialog.getByRole('heading', { name: 'No lyrics available for this song.' }),
      ).toBeVisible();
    else {
      await expect(dialog.getByRole('alert')).toContainText('Try again');
      await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
      await expect(dialog.locator('.lyrics-copy')).toContainText('First fixture song');
    }
    await page.mouse.click(5, 5);
    await expect(dialog).toHaveCount(0);
  });
