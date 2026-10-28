// Plain text uses declared ink against rendered backgrounds. Shadow-backed text
// uses actual filled glyph cores against a second render retaining the halo.
export async function renderedTextContrast(page, selectors) {
  const samples = await page.evaluate((selectors) => {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 1;
    const context = canvas.getContext('2d');
    const samples = [];
    let nextId = 0;
    for (const { selector, minimum = 4.5, pseudo = null, includeDisabled = false } of selectors) {
      for (const node of document.querySelectorAll(selector)) {
        const box = node.getBoundingClientRect();
        if (
          box.width < 4 ||
          box.height < 4 ||
          box.bottom <= 0 ||
          box.top >= innerHeight ||
          (!includeDisabled && node.closest('[disabled]'))
        )
          continue;
        node.setAttribute('data-contrast-target', 'true');
        const text = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
        const ranges = [];
        if (pseudo)
          ranges.push({ rect: box, element: node, text: node.getAttribute('placeholder') ?? '' });
        else if (node instanceof HTMLInputElement && node.value)
          ranges.push({ rect: box, element: node, text: node.value });
        while (!pseudo && text.nextNode()) {
          if (!text.currentNode.textContent.trim()) continue;
          const range = document.createRange();
          range.selectNodeContents(text.currentNode);
          for (const rect of range.getClientRects())
            ranges.push({
              rect,
              element: text.currentNode.parentElement,
              text: text.currentNode.textContent.trim(),
            });
        }
        for (const range of ranges) {
          const style = getComputedStyle(range.element, pseudo);
          context.clearRect(0, 0, 1, 1);
          context.fillStyle = style.webkitTextFillColor || style.color;
          context.fillRect(0, 0, 1, 1);
          const color = [...context.getImageData(0, 0, 1, 1).data];
          let opacity = 1;
          for (let ancestor = range.element; ancestor; ancestor = ancestor.parentElement)
            opacity *= Number(getComputedStyle(ancestor).opacity);
          color[3] = (color[3] / 255) * opacity;
          let x = Math.max(box.left, range.rect.left),
            y = Math.max(box.top, range.rect.top);
          let right = Math.min(box.right, range.rect.right),
            bottom = Math.min(box.bottom, range.rect.bottom);
          for (let ancestor = node.parentElement; ancestor; ancestor = ancestor.parentElement) {
            const clip = ancestor.getBoundingClientRect(),
              style = getComputedStyle(ancestor);
            if (style.overflowX !== 'visible') {
              x = Math.max(x, clip.left);
              right = Math.min(right, clip.right);
            }
            if (style.overflowY !== 'visible') {
              y = Math.max(y, clip.top);
              bottom = Math.min(bottom, clip.bottom);
            }
          }
          const width = right - x,
            height = bottom - y;
          const id = range.element.getAttribute('data-contrast-ink') ?? String(nextId++);
          range.element.setAttribute('data-contrast-ink', id);
          if (width >= 4 && height >= 4)
            samples.push({
              selector,
              elementId: id,
              text: range.text.slice(0, 35),
              color,
              shadowed: style.textShadow !== 'none',
              minimum,
              x,
              y,
              width,
              height,
            });
        }
      }
    }
    return samples;
  }, selectors);
  const visible = samples.some((sample) => sample.shadowed)
    ? await page.screenshot({ animations: 'disabled' })
    : null;
  const hidden = await page.addStyleTag({
    content: `
    [data-contrast-target], [data-contrast-target] *, [data-contrast-target]::placeholder {
      color: transparent !important; -webkit-text-fill-color: transparent !important;
      -webkit-text-stroke-width: 0 !important;
    }
  `,
  });
  try {
    const png = await page.screenshot({ animations: 'disabled' });
    return await page.evaluate(
      async ({ png, visible, samples }) => {
        const image = new Image();
        image.src = `data:image/png;base64,${png}`;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext('2d');
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
        let glyphs = null;
        if (visible) {
          const ink = new Image();
          ink.src = `data:image/png;base64,${visible}`;
          await ink.decode();
          context.clearRect(0, 0, canvas.width, canvas.height);
          context.drawImage(ink, 0, 0);
          glyphs = context.getImageData(0, 0, canvas.width, canvas.height).data;
        }
        const sx = canvas.width / innerWidth,
          sy = canvas.height / innerHeight;
        const luminance = (color) =>
          color
            .map((value) => {
              const channel = value / 255;
              return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
            })
            .reduce((sum, value, i) => sum + value * [0.2126, 0.7152, 0.0722][i], 0);
        const contrast = (a, b) => {
          const x = luminance(a),
            y = luminance(b);
          return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
        };
        return samples
          .map((sample) => {
            const element = document.querySelector(`[data-contrast-ink="${sample.elementId}"]`);
            let ratio = Infinity;
            for (
              let y = Math.max(0, Math.ceil((sample.y + 2) * sy));
              y < Math.min(canvas.height, (sample.y + sample.height - 2) * sy);
              y += sample.shadowed ? 1 : 2
            ) {
              for (
                let x = Math.max(0, Math.ceil((sample.x + 2) * sx));
                x < Math.min(canvas.width, (sample.x + sample.width - 2) * sx);
                x += sample.shadowed ? 1 : 2
              ) {
                const offset = (y * canvas.width + x) * 4;
                const background = [pixels[offset], pixels[offset + 1], pixels[offset + 2]];
                const foreground = background.map(
                  (value, i) => sample.color[i] * sample.color[3] + value * (1 - sample.color[3]),
                );
                let visibleContrast;
                if (sample.shadowed && glyphs) {
                  const painted = [glyphs[offset], glyphs[offset + 1], glyphs[offset + 2]];
                  const difference = Math.max(
                    ...painted.map((value, i) => Math.abs(value - background[i])),
                  );
                  const full = Math.max(
                    ...foreground.map((value, i) => Math.abs(value - background[i])),
                  );
                  if (difference < 25 || !full || difference / full < 0.7) continue;
                  visibleContrast = contrast(painted, background);
                } else {
                  visibleContrast = contrast(foreground, background);
                }
                const hit = document.elementFromPoint(x / sx, y / sy);
                if (!hit || !element.contains(hit)) continue;
                ratio = Math.min(ratio, visibleContrast);
              }
            }
            if (sample.shadowed && !Number.isFinite(ratio)) ratio = 0;
            return Number.isFinite(ratio)
              ? {
                  selector: sample.selector,
                  text: sample.text,
                  ratio,
                  minimum: sample.minimum,
                  shadowed: sample.shadowed,
                }
              : null;
          })
          .filter(Boolean);
      },
      { png: png.toString('base64'), visible: visible?.toString('base64') ?? null, samples },
    );
  } finally {
    await hidden.evaluate((node) => node.remove());
    await page.evaluate(() =>
      document
        .querySelectorAll('[data-contrast-target]')
        .forEach((node) => node.removeAttribute('data-contrast-target')),
    );
    await page.evaluate(() =>
      document
        .querySelectorAll('[data-contrast-ink]')
        .forEach((node) => node.removeAttribute('data-contrast-ink')),
    );
  }
}
