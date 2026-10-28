import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';
import { renderedTextContrast } from './contrast';

for (const colorMode of ['light', 'dark']) {
  for (const tint of [50, 69, 70, 75, 90, 100]) {
    test(`${colorMode} glass text contrast stays readable over desktop fixtures at ${tint}% tint`, async ({
      page,
    }, testInfo) => {
      await nativeFixture(page);
      await page.emulateMedia({ reducedMotion: 'reduce' });
      await page.setViewportSize({ width: 1400, height: 1000 });
      await page.addInitScript(
        ({ tint, colorMode }) =>
          localStorage.setItem(
            'vessel:appearance:v2',
            JSON.stringify({
              version: 2,
              colorMode,
              accent: 'rose',
              glassOpacity: tint,
              density: 'comfortable',
              sidebarCollapsed: false,
              showNowPlaying: true,
              reducedTransparency: false,
            }),
          ),
        { tint, colorMode },
      );
      await page.goto('/');
      await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
      await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
      await page.addStyleTag({
        content: `html { background: var(--test-desktop, #fff) !important; } body { background: transparent !important; }`,
      });
      const common = [
        { selector: '.sidebar-label' },
        { selector: '.sidebar nav > button > span' },
        { selector: '.session-label' },
        { selector: '.page-breadcrumb' },
        { selector: '.now-playing h2, .now-playing p' },
        { selector: '.artwork-track-heading h2, .artwork-track-heading p' },
        { selector: '.artwork-footer, .artwork-source-quality' },
      ];
      const desktops = [
        ['bright', '#fff'],
        ['dark', '#080808'],
        ['busy', 'repeating-conic-gradient(#fff 0% 25%, #111 0% 50%) 0 0 / 18px 18px'],
      ];
      for (const view of ['home', 'search']) {
        if (view === 'search') {
          await page.getByRole('searchbox', { name: 'Search music' }).fill('Daft Punk');
          await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
          await expect(page.locator('.song-artist').first()).toBeVisible();
        }
        const selectors = [
          ...common,
          ...(view === 'home'
            ? [
                { selector: '.provider-home-heading h1', minimum: 3 },
                { selector: '.provider-home-heading p:last-child, .feed-scope' },
                { selector: '.provider-shelf-heading h2' },
                { selector: '.search-field input', pseudo: '::placeholder' },
                { selector: '.provider-card > strong, .provider-card-subtitle' },
              ]
            : [
                { selector: '.page-heading h1', minimum: 3 },
                { selector: '.search-field input' },
                { selector: '.page-heading > p:last-child, .result-count' },
                { selector: '.filter-pills button' },
                { selector: '.song-title, .song-artist, .album-cell, .time-cell' },
              ]),
        ];
        for (const [name, background] of desktops) {
          await page.evaluate(
            (background) =>
              document.documentElement.style.setProperty('--test-desktop', background),
            background,
          );
          await page.screenshot({ path: testInfo.outputPath(`glass-${view}-${name}.png`) });
          const samples = await renderedTextContrast(page, selectors);
          expect(samples.length).toBeGreaterThan(10);
          const failures = samples.filter((sample) => sample.ratio + 0.01 < sample.minimum);
          console.log(
            `${colorMode}/${tint}% ${view}/${name}: minimum ${Math.min(...samples.map((sample) => sample.ratio)).toFixed(2)}:1`,
          );
          expect(failures.length, `${view}/${name}: ${JSON.stringify(failures.slice(0, 12))}`).toBe(
            0,
          );
        }
      }
    });
  }
}

for (const colorMode of ['light', 'dark']) {
  test(`${colorMode} glass controls keep contrast across accents and honor increased contrast without changing preferences`, async ({
    page,
  }) => {
    await nativeFixture(page, { googleRejected: true });
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.setViewportSize({ width: 1400, height: 1000 });
    await page.addInitScript(
      (colorMode) =>
        localStorage.setItem(
          'vessel:appearance:v2',
          JSON.stringify({ version: 2, colorMode, glassOpacity: 50 }),
        ),
      colorMode,
    );
    await page.goto('/');
    await page.getByRole('button', { name: 'Play Instant Crush', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Pause playback', exact: true })).toBeEnabled();
    await page.addStyleTag({
      content: 'html { background: #fff !important; } body { background: transparent !important; }',
    });
    await page.getByRole('searchbox', { name: 'Search music' }).fill('Daft Punk');
    await page.getByRole('searchbox', { name: 'Search music' }).press('Enter');
    await expect(page.locator('.song-artist').first()).toBeVisible();
    for (const tint of [90, 50]) {
      for (const accent of ['Rose', 'Iris', 'Foam', 'Gold']) {
        await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
        await page.getByRole('button', { name: `${accent} accent`, exact: true }).click();
        await page
          .getByRole('slider', { name: 'Glass tint', exact: true })
          .evaluate((node, tint) => {
            node.value = String(tint);
            node.dispatchEvent(new Event('input', { bubbles: true }));
            node.dispatchEvent(new Event('change', { bubbles: true }));
          }, tint);
        await page.getByRole('button', { name: 'Close appearance settings', exact: true }).click();
        await expect
          .poll(() =>
            page
              .locator('.app-frame')
              .evaluate((node) => getComputedStyle(node).getPropertyValue('--glass-opacity')),
          )
          .toBe(String(tint / 100));
        const samples = await renderedTextContrast(page, [
          { selector: '.filter-pills button, .song-title, .song-artist, .result-count' },
        ]);
        const failures = samples.filter((sample) => sample.ratio + 0.01 < sample.minimum);
        expect(failures.length, `${accent}/${tint}: ${JSON.stringify(failures)}`).toBe(0);
      }
    }
    const saved = await page.evaluate(() => localStorage.getItem('vessel:appearance:v2'));
    await page.emulateMedia({ reducedMotion: 'reduce', contrast: 'more' });
    expect(
      await page.locator('.app-frame').evaluate((node) => {
        const style = getComputedStyle(node);
        return (
          style.getPropertyValue('--theme-secondary').trim() ===
          style.getPropertyValue('--theme-foreground').trim()
        );
      }),
    ).toBe(true);
    expect(await page.evaluate(() => localStorage.getItem('vessel:appearance:v2'))).toBe(saved);
    const increased = await renderedTextContrast(page, [
      { selector: '.song-title, .song-artist, .filter-pills button' },
    ]);
    expect(increased.length).toBeGreaterThan(5);
    expect(increased.filter((sample) => sample.ratio + 0.01 < sample.minimum)).toEqual([]);
    await page.getByRole('button', { name: 'Open account settings', exact: true }).click();
    await page.getByRole('button', { name: 'Connect Google', exact: true }).click();
    await expect(page.locator('.account-error')).toBeVisible();
    await page.emulateMedia({ reducedMotion: 'reduce', contrast: 'no-preference' });
    const account = await renderedTextContrast(page, [
      {
        selector:
          '.account-description, .account-caption, .account-privacy p, .account-error, .connect-google',
      },
    ]);
    expect(account.length).toBeGreaterThan(3);
    expect(account.filter((sample) => sample.ratio + 0.01 < sample.minimum)).toEqual([]);
    await page.getByRole('button', { name: 'Close account settings', exact: true }).click();
    await page.evaluate(() => window.__vesselAdvanceRadio());
    await page.getByRole('button', { name: 'Show play queue', exact: true }).click();
    await expect(page.locator('.queue-tracks li.past')).toHaveCount(1);
    const queue = await renderedTextContrast(page, [
      {
        selector:
          '.queue-copy strong, .queue-copy small, .queue-duration, .queue-footer, .queue-seed',
      },
    ]);
    expect(queue.length).toBeGreaterThan(5);
    expect(queue.filter((sample) => sample.ratio + 0.01 < sample.minimum)).toEqual([]);
    await page.getByRole('button', { name: 'Close play queue', exact: true }).click();
    await page.getByRole('button', { name: 'Audio quality', exact: true }).click();
    const quality = await renderedTextContrast(page, [
      { selector: '.quality-option, .quality-popover > p' },
    ]);
    expect(quality.length).toBeGreaterThan(1);
    expect(quality.filter((sample) => sample.ratio + 0.01 < sample.minimum)).toEqual([]);
  });
}
