// Live vector clips avoid the unpainted frames caused by decoding replacement
// mask images. Complements are intersected so overlapping holes remain holes.
interface Bounds {
  x: number;
  y: number;
  width: number;
  height: number;
  radius: number;
  scaleX: number;
  scaleY: number;
}

let instance = 0;
const ns = 'http://www.w3.org/2000/svg';

export function unifiedMaterial(root: HTMLElement) {
  const prefix = `vessel-material-${++instance}`;
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('width', '0');
  svg.setAttribute('height', '0');
  svg.setAttribute('aria-hidden', 'true');
  svg.style.position = 'absolute';
  svg.style.pointerEvents = 'none';
  const defs = document.createElementNS(ns, 'defs');
  svg.append(defs);
  root.append(svg);
  const clips = new Map<HTMLElement, { nodes: SVGClipPathElement[]; key: string; id: string }>();
  let serial = 0;
  let disposed = false;
  let frame = 0;
  const watchedAnimations = new WeakSet<Animation>();
  const observed = new Set<HTMLElement>();
  const resize = new ResizeObserver(schedule);
  const mutation = new MutationObserver((records) => {
    if (records.some((record) => !svg.contains(record.target))) schedule();
  });

  function schedule() {
    if (!disposed && !frame) frame = requestAnimationFrame(update);
  }

  function setClip(
    node: HTMLElement,
    width: number,
    height: number,
    holes: Omit<Bounds, 'scaleX' | 'scaleY'>[],
  ) {
    let value = 'none';
    if (holes.length) {
      let clip = clips.get(node);
      if (!clip) {
        clip = { nodes: [], key: '', id: `${prefix}-${++serial}` };
        clips.set(node, clip);
      }
      const key = JSON.stringify([width, height, holes]);
      if (key !== clip.key) {
        while (clip.nodes.length > holes.length) clip.nodes.pop()?.remove();
        holes.forEach((hole, index) => {
          let shape = clip.nodes[index];
          if (!shape) {
            shape = document.createElementNS(ns, 'clipPath');
            shape.id = `${clip.id}-${index}`;
            shape.setAttribute('clipPathUnits', 'userSpaceOnUse');
            defs.append(shape);
            clip.nodes.push(shape);
          }
          const { x, y, width: w, height: h } = hole;
          const r = Math.max(0, Math.min(hole.radius, w / 2, h / 2));
          const path = document.createElementNS(ns, 'path');
          path.setAttribute('clip-rule', 'evenodd');
          path.setAttribute(
            'd',
            `M0 0H${width}V${height}H0Z M${x + r} ${y}H${x + w - r}A${r} ${r} 0 0 1 ${x + w} ${y + r}V${y + h - r}A${r} ${r} 0 0 1 ${x + w - r} ${y + h}H${x + r}A${r} ${r} 0 0 1 ${x} ${y + h - r}V${y + r}A${r} ${r} 0 0 1 ${x + r} ${y}Z`,
          );
          if (index) path.setAttribute('clip-path', `url(#${clip.id}-${index - 1})`);
          shape.replaceChildren(path);
        });
        clip.key = key;
      }
      value = `url("#${clip.id}-${holes.length - 1}")`;
    }
    if (node.style.getPropertyValue('--material-clip') !== value)
      node.style.setProperty('--material-clip', value);
  }

  function update() {
    frame = 0;
    const box = root.getBoundingClientRect();
    const overlays = [...root.querySelectorAll<HTMLElement>('[data-material-overlay]')].filter(
      (node) => ['absolute', 'fixed'].includes(getComputedStyle(node).position),
    );
    const current = new Set(overlays);
    let animating = false;
    for (const node of observed) {
      if (!current.has(node)) {
        resize.unobserve(node);
        observed.delete(node);
        node.style.removeProperty('--material-clip');
        clips.get(node)?.nodes.forEach((clip) => clip.remove());
        clips.delete(node);
      }
    }
    const bounds: Bounds[] = overlays.map((node) => {
      if (!observed.has(node)) {
        resize.observe(node);
        observed.add(node);
      }
      for (const animation of node.getAnimations()) {
        animating ||= animation.playState === 'running';
        if (!watchedAnimations.has(animation)) {
          watchedAnimations.add(animation);
          void animation.finished.then(schedule, schedule);
        }
      }
      const rect = node.getBoundingClientRect();
      const scaleX = node.offsetWidth ? rect.width / node.offsetWidth : 1;
      const scaleY = node.offsetHeight ? rect.height / node.offsetHeight : 1;
      // ::before paints the padding box, not the outer border box.
      return {
        x: rect.left - box.left + node.clientLeft * scaleX,
        y: rect.top - box.top + node.clientTop * scaleY,
        width: node.clientWidth * scaleX,
        height: node.clientHeight * scaleY,
        radius: (parseFloat(getComputedStyle(node).borderTopLeftRadius) || 0) * scaleX,
        scaleX,
        scaleY,
      };
    });
    setClip(root, root.clientWidth, root.clientHeight, bounds);
    overlays.forEach((node, index) => {
      const own = bounds[index];
      setClip(
        node,
        node.clientWidth,
        node.clientHeight,
        bounds
          .slice(index + 1)
          .filter(
            (upper) =>
              upper.x < own.x + own.width &&
              upper.x + upper.width > own.x &&
              upper.y < own.y + own.height &&
              upper.y + upper.height > own.y,
          )
          .map((upper) => ({
            x: (upper.x - own.x) / own.scaleX,
            y: (upper.y - own.y) / own.scaleY,
            width: upper.width / own.scaleX,
            height: upper.height / own.scaleY,
            radius: upper.radius / own.scaleX,
          })),
      );
    });
    if (animating) schedule();
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
      root.style.removeProperty('--material-clip');
      for (const node of observed) node.style.removeProperty('--material-clip');
      svg.remove();
    },
  };
}
