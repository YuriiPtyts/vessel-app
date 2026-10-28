// Each visible point gets one tint layer. Cut the window's material out beneath
// floating surfaces, and cut lower floating materials out beneath higher ones.
// Only element bounds are observed; desktop pixels are never read or captured.
interface Bounds {
  x: number;
  y: number;
  width: number;
  height: number;
  radius: number;
}

function coverageMask(width: number, height: number, holes: Bounds[]): string {
  if (!holes.length) return 'none';
  const cuts = holes
    .map(
      ({ x, y, width, height, radius }) =>
        `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="black"/>`,
    )
    .join('');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><defs><mask id="coverage"><rect width="100%" height="100%" fill="white"/>${cuts}</mask></defs><rect width="100%" height="100%" fill="white" mask="url(#coverage)"/></svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
}

export function unifiedMaterial(root: HTMLElement) {
  let disposed = false;
  let frame = 0;
  const watchedAnimations = new WeakSet<Animation>();
  const observed = new Set<HTMLElement>();
  const resize = new ResizeObserver(schedule);
  const mutation = new MutationObserver(schedule);

  function schedule() {
    if (!disposed && !frame) frame = requestAnimationFrame(update);
  }

  function update() {
    frame = 0;
    const box = root.getBoundingClientRect();
    const overlays = [...root.querySelectorAll<HTMLElement>('[data-material-overlay]')].filter(
      (node) => ['absolute', 'fixed'].includes(getComputedStyle(node).position),
    );
    const current = new Set(overlays);
    for (const node of observed) {
      if (!current.has(node)) {
        resize.unobserve(node);
        observed.delete(node);
        node.style.removeProperty('--material-mask');
      }
    }
    const bounds = overlays.map((node) => {
      if (!observed.has(node)) {
        resize.observe(node);
        observed.add(node);
      }
      for (const animation of node.getAnimations()) {
        if (!watchedAnimations.has(animation)) {
          watchedAnimations.add(animation);
          void animation.finished.then(schedule, schedule);
        }
      }
      const rect = node.getBoundingClientRect();
      return {
        x: rect.left - box.left,
        y: rect.top - box.top,
        width: rect.width,
        height: rect.height,
        radius: parseFloat(getComputedStyle(node).borderTopLeftRadius) || 0,
      };
    });
    function setMask(node: HTMLElement, width: number, height: number, holes: Bounds[]) {
      const mask = coverageMask(width, height, holes);
      if (node.style.getPropertyValue('--material-mask') !== mask)
        node.style.setProperty('--material-mask', mask);
    }
    setMask(root, box.width, box.height, bounds);
    overlays.forEach((node, index) => {
      const own = bounds[index];
      setMask(
        node,
        own.width,
        own.height,
        bounds
          .slice(index + 1)
          .filter(
            (upper) =>
              upper.x < own.x + own.width &&
              upper.x + upper.width > own.x &&
              upper.y < own.y + own.height &&
              upper.y + upper.height > own.y,
          )
          .map((upper) => ({ ...upper, x: upper.x - own.x, y: upper.y - own.y })),
      );
    });
  }

  resize.observe(root);
  mutation.observe(root, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['class'],
  });
  schedule();
  return {
    destroy() {
      disposed = true;
      cancelAnimationFrame(frame);
      resize.disconnect();
      mutation.disconnect();
      root.style.removeProperty('--material-mask');
      for (const node of observed) node.style.removeProperty('--material-mask');
    },
  };
}
