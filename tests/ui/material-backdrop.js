// Browser previews cannot render AppKit's native Regular glass or DWM Acrylic.
// This controlled underlay exercises plain text against a luminosity-supported
// backdrop. 35% dimming comes from Apple's clear-glass legibility guidance; it
// is a test fixture, not an exact model or measurement of the native compositor.
export async function materialBackdrop(page) {
  await page.evaluate(() => {
    const backing = document.createElement('div');
    backing.id = 'native-material-fixture';
    backing.setAttribute('aria-hidden', 'true');
    document.body.prepend(backing);
  });
  await page.addStyleTag({
    content: `
    #native-material-fixture { position: fixed; inset: 0; z-index: 0; pointer-events: none; }
    body:has(.app-frame:not(.light-glass)) #native-material-fixture { background: rgb(0 0 0 / 35%); }
    #app { position: relative; z-index: 1; }
  `,
  });
}
