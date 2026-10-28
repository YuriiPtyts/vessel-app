import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';

async function connect(page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect(page.getByRole('button', { name: 'Saved discoveries', exact: true })).toBeVisible();
}

test('account hearts write to YouTube Music and sign-out clears the account collection', async ({
  page,
}) => {
  await nativeFixture(page, { deviceLikes: true });
  await connect(page);
  await page.getByRole('button', { name: /^Liked songs/ }).click();
  await expect(page.getByRole('button', { name: 'Play Da Funk', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toHaveCount(
    0,
  );
  await page.getByRole('button', { name: 'Unsave Da Funk', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Play Da Funk', exact: true })).toHaveCount(0);
  await page.getByRole('searchbox', { name: 'Search music' }).fill('Daft Punk');
  await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
  const save = page.getByRole('button', { name: 'Save Instant Crush', exact: true });
  await expect(save).toHaveAttribute('title', /YouTube Music/);
  await save.click();
  await expect(
    page.getByRole('button', { name: 'Unsave Instant Crush', exact: true }),
  ).toBeEnabled();
  expect(
    await page.evaluate(() =>
      window.__vesselCalls
        .filter((call) => call.command === 'set_account_like')
        .map((call) => call.args),
    ),
  ).toEqual([
    { videoId: 'd7woYjGWs6k', liked: false, accountRevision: 1 },
    { videoId: 'khnokW3Mw24', liked: true, accountRevision: 1 },
  ]);
  expect(
    await page.evaluate(() =>
      window.__vesselCalls.filter((call) => call.command === 'toggle_like'),
    ),
  ).toEqual([]);
  await expect(page.getByRole('button', { name: /^Device likes/ })).toHaveCount(0);
  await page.getByRole('button', { name: 'Open account settings', exact: true }).click();
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await page.getByRole('button', { name: 'Close account settings', exact: true }).click();
  await page.getByRole('button', { name: /^Liked songs/ }).click();
  await expect(
    page.getByRole('heading', { name: 'Connect to see your liked songs.' }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toHaveCount(
    0,
  );
  await expect(page.getByRole('button', { name: 'Saved discoveries', exact: true })).toHaveCount(0);
});

test('saved sidebar playlists paginate and seed playback with their displayed order', async ({
  page,
}) => {
  await nativeFixture(page);
  await connect(page);
  await page.getByRole('button', { name: 'Saved discoveries', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Saved discoveries', exact: true })).toBeVisible();
  await expect(page.locator('.track-table tbody tr')).toHaveCount(2);
  await page.getByRole('button', { name: 'Show more songs', exact: true }).click();
  await expect(page.locator('.track-table tbody tr')).toHaveCount(5);
  await page.screenshot({ path: test.info().outputPath('account-playlist.png') });
  await page.setViewportSize({ width: 420, height: 800 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: test.info().outputPath('account-playlist-narrow.png') });
  await page.getByRole('button', { name: 'Play Around the World', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  const calls = await page.evaluate(() => window.__vesselCalls);
  expect(
    calls.filter((call) => call.command === 'account_playlist').map((call) => call.args),
  ).toEqual([
    { id: 'VLaccount_saved', accountRevision: 1, more: false },
    { id: 'VLaccount_saved', accountRevision: 1, more: true },
  ]);
  expect(calls.find((call) => call.command === 'play_song').args.context).toEqual([
    'khnokW3Mw24',
    'd7woYjGWs6k',
    'LxgnIhjQFuE',
    'lw19BL_2e_Q',
    '9WkE0L5TQT4',
  ]);
});

test('refresh reconciles remote likes and rejects snapshots from previous accounts', async ({
  page,
}) => {
  await nativeFixture(page);
  await connect(page);
  await page.getByRole('button', { name: /^Liked songs/ }).click();
  await expect(page.getByRole('button', { name: 'Play Da Funk', exact: true })).toBeVisible();
  await page.evaluate(() => window.__vesselSetRemoteLikes([2]));
  await page.getByRole('button', { name: 'Refresh likes', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Play Harder, Better, Faster, Stronger', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play Da Funk', exact: true })).toHaveCount(0);
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect(page.getByRole('button', { name: 'Refresh likes', exact: true })).toBeEnabled();
  await page.evaluate(() =>
    window.__vesselEmitAccountLibrary({
      revision: 999,
      accountRevision: 1,
      connected: true,
      refreshing: false,
      likesReady: true,
      playlistsReady: true,
      liked: [],
      likedIds: [],
      playlists: [],
      likesError: null,
      playlistsError: null,
    }),
  );
  await expect(
    page.getByRole('button', { name: 'Play Harder, Better, Faster, Stronger', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Saved discoveries', exact: true })).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.__vesselCalls.some(
        (call) => call.command === 'refresh_account_library' && call.args.force,
      ),
    ),
  ).toBe(true);
});

test('a late refresh after sign-out cannot restore account playlists or likes', async ({
  page,
}) => {
  await nativeFixture(page, { deviceLikes: true, libraryDelay: 300 });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toBeVisible();
  await page.evaluate(() => window.__vesselCompleteSignIn());
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.__vesselCalls.some((call) => call.command === 'refresh_account_library'),
      ),
    )
    .toBe(true);
  await page.getByRole('button', { name: 'Open account settings', exact: true }).click();
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await page.getByRole('button', { name: 'Close account settings', exact: true }).click();
  await page.getByRole('button', { name: /^Liked songs/ }).click();
  await expect(
    page.getByRole('heading', { name: 'Connect to see your liked songs.' }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toHaveCount(
    0,
  );
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__vesselCalls.filter((call) => call.command === 'refresh_account_library').length,
      ),
    )
    .toBe(1);
  await expect.poll(() => page.evaluate(() => window.__vesselLibraryReplies)).toBe(1);
  await expect(page.getByRole('button', { name: 'Saved discoveries', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Play Instant Crush', exact: true })).toHaveCount(
    0,
  );
  await expect(page.getByRole('button', { name: 'Play Da Funk', exact: true })).toHaveCount(0);
});

test('failed account synchronization is distinct from an empty collection and playback remains usable', async ({
  page,
}) => {
  await nativeFixture(page, { deviceLikes: true, likesFailure: true });
  await page.setViewportSize({ width: 1400, height: 1000 });
  await connect(page);
  await page.getByRole('button', { name: /^Liked songs/ }).click();
  await expect(page.getByRole('alert')).toContainText('could not sync');
  await expect(
    page.getByRole('heading', { name: 'Your account likes are not available yet.' }),
  ).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Make room for your favorites.' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Listen now', exact: true }).click();
  await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Save current song', exact: true })).toBeDisabled();
  expect(
    await page.evaluate(() => window.__vesselCalls.some((call) => call.command === 'toggle_like')),
  ).toBe(false);
});

test('a rejected provider write leaves the heart and liked song unchanged', async ({ page }) => {
  await nativeFixture(page, { likeFailure: true });
  await connect(page);
  await page.getByRole('button', { name: /^Liked songs/ }).click();
  await page.getByRole('button', { name: 'Unsave Da Funk', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('did not confirm');
  await expect(page.getByRole('button', { name: 'Unsave Da Funk', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Play Da Funk', exact: true })).toBeVisible();
});
