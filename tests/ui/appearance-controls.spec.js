import { test, expect } from '@playwright/test';
import { nativeFixture } from './native-fixture';
import { renderedTextContrast } from './contrast';
import { materialBackdrop } from './material-backdrop';

async function materialColor(page) {
  return page.locator('.app-frame').evaluate((node) =>
    getComputedStyle(node, '::before')
      .backgroundColor.match(/[\d.]+/g)
      .slice(0, 3)
      .map(Number),
  );
}

async function setTint(page, value) {
  await page.getByRole('slider', { name: 'Glass tint', exact: true }).evaluate((node, value) => {
    node.value = String(value);
    node.dispatchEvent(new Event('input', { bubbles: true }));
    node.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}

test('glass tint changes alpha without changing the chosen material color', async ({ page }) => {
  await nativeFixture(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
  const original = await materialColor(page);
  for (const tint of [50, 69, 70, 85, 100]) {
    await setTint(page, tint);
    expect(await materialColor(page), `tint ${tint} must not switch light/dark material`).toEqual(
      original,
    );
    const alpha = await page.locator('.app-frame').evaluate((node) => {
      const color = getComputedStyle(node, '::before')
        .backgroundColor.match(/[\d.]+/g)
        .map(Number);
      return color.length === 4 ? color[3] : 1;
    });
    expect(alpha).toBeCloseTo(tint / 100, 2);
  }
});

test('light and dark appearance are separate controls and survive a restart', async ({ page }) => {
  await nativeFixture(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
  await setTint(page, 85);
  const dark = page.getByRole('button', { name: 'Dark appearance', exact: true });
  const light = page.getByRole('button', { name: 'Light appearance', exact: true });
  await expect(dark).toBeVisible();
  await dark.click();
  const darkColor = await materialColor(page);
  await light.click();
  expect(await materialColor(page)).not.toEqual(darkColor);
  await expect(page.getByRole('slider', { name: 'Glass tint', exact: true })).toHaveValue('85');
  await page.reload();
  await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
  await expect(light).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('slider', { name: 'Glass tint', exact: true })).toHaveValue('85');
  await page.getByRole('button', { name: 'Close appearance settings', exact: true }).click();
  await page.getByRole('button', { name: 'Switch to dark appearance', exact: true }).click();
  expect(await materialColor(page)).toEqual(darkColor);
  await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
  await expect(dark).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('slider', { name: 'Glass tint', exact: true })).toHaveValue('85');
});

for (const tint of [50, 80]) {
  test(`legacy ${tint}% appearance is kept when adding the explicit light/dark choice`, async ({
    page,
  }) => {
    await nativeFixture(page);
    await page.addInitScript(
      (glassOpacity) =>
        localStorage.setItem(
          'vessel:appearance:v2',
          JSON.stringify({
            version: 2,
            glassOpacity,
            accent: 'foam',
            density: 'compact',
            sidebarCollapsed: true,
            showNowPlaying: false,
            reducedTransparency: false,
          }),
        ),
      tint,
    );
    await page.goto('/');
    await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
    await expect(
      page.getByRole('button', {
        name: tint < 70 ? 'Light appearance' : 'Dark appearance',
        exact: true,
      }),
    ).toHaveAttribute('aria-pressed', 'true');
    const original = await materialColor(page);
    await setTint(page, tint < 70 ? 90 : 50);
    expect(await materialColor(page)).toEqual(original);
    await expect(page.locator('.app-frame')).toHaveClass(/accent-foam/);
    await expect(page.locator('.app-frame')).toHaveClass(/density-compact/);
    await expect(page.locator('.app-frame')).toHaveClass(/sidebar-compact/);
  });
}

test('dark low-tint typography stays crisp without outlines, halos, or heavier weights', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await page.addStyleTag({
    content: 'html { background: #fff !important; } body { background: transparent !important; }',
  });
  await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
  await page.getByRole('button', { name: 'Dark appearance', exact: true }).click();
  await setTint(page, 85);
  const fixedColors = () =>
    page.evaluate(() => ({
      material: getComputedStyle(document.querySelector('.app-frame'), '::before')
        .backgroundColor.match(/[\d.]+/g)
        .slice(0, 3),
      icon: getComputedStyle(document.querySelector('.brand-symbol')).color,
      border: getComputedStyle(document.querySelector('.sidebar')).borderRightColor,
      text: getComputedStyle(document.querySelector('.provider-home-heading h1')).color,
    }));
  const original = await fixedColors();
  const typography = () =>
    page
      .locator(
        '.provider-home-heading h1, .provider-home-heading p:last-child, .sidebar nav > button > span, .search-field input',
      )
      .evaluateAll((nodes) =>
        nodes.map((node) => {
          const ink = getComputedStyle(node);
          return {
            weight: ink.fontWeight,
            fill: ink.webkitTextFillColor,
            color: ink.color,
            shadow: ink.textShadow,
            stroke: ink.webkitTextStrokeWidth,
          };
        }),
      );
  const originalTypography = await typography();
  await setTint(page, 50);
  expect(await fixedColors()).toEqual(original);
  await page.screenshot({ path: test.info().outputPath('low-tint-dark.png') });
  const heading = page.locator('.provider-home-heading h1');
  expect(
    await heading.evaluate((node) => parseFloat(getComputedStyle(node).webkitTextStrokeWidth)),
    'small text must retain a filled center, not a thin outline',
  ).toBe(0);
  expect(
    await heading.evaluate((node) => getComputedStyle(node).textShadow),
    'No contour or halo around the letters',
  ).toBe('none');
  expect(await typography()).toEqual(originalTypography);
  await page.getByRole('button', { name: 'Close appearance settings', exact: true }).click();
  await materialBackdrop(page);
  await page.screenshot({ path: test.info().outputPath('low-tint-regular-backed.png') });
  const selectors = [
    { selector: '.provider-home-heading p:last-child, .sidebar nav > button > span' },
  ];
  const supported = await renderedTextContrast(page, selectors);
  expect(supported.every((sample) => sample.ratio >= sample.minimum)).toBe(true);
  await page.locator('#native-material-fixture').evaluate((node) => node.remove());
  const unprotected = await renderedTextContrast(page, selectors);
  expect(unprotected.some((sample) => sample.ratio < sample.minimum)).toBe(true);
});

test('opening a panel keeps its width and height in proportion throughout the reveal', async ({
  page,
}) => {
  await nativeFixture(page);
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.addInitScript(() => {
    const original = Element.prototype.animate;
    window.__vesselBalancedReveal = null;
    Element.prototype.animate = function (...args) {
      const animation = original.apply(this, args);
      if (
        this.matches('.appearance-popover') &&
        Number(animation.effect.getTiming().duration) > 0
      ) {
        animation.pause();
        window.__vesselBalancedReveal = animation;
      }
      return animation;
    };
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Appearance settings', exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.__vesselBalancedReveal))).toBe(true);
  for (const progress of [0.25, 0.5, 0.75]) {
    const scale = await page.locator('.appearance-popover').evaluate((node, progress) => {
      const animation = window.__vesselBalancedReveal;
      animation.currentTime = Number(animation.effect.getTiming().duration) * progress;
      const box = node.getBoundingClientRect();
      return {
        x: box.width / node.offsetWidth,
        y: box.height / node.offsetHeight,
        materialOpacity: Number(getComputedStyle(node).opacity),
      };
    }, progress);
    expect(Math.abs(scale.x - scale.y), `${progress}: ${JSON.stringify(scale)}`).toBeLessThan(0.03);
    expect(Math.min(scale.x, scale.y)).toBeGreaterThan(0.9);
    expect(
      scale.materialOpacity,
      'the material must keep its alpha and backdrop blur while content reveals',
    ).toBe(1);
  }
});
