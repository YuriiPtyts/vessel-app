import { cubicOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

type GenieOrigin = 'bottom-left' | 'bottom-right' | 'top-right';

/** A short, anchored genie reveal for transient surfaces, never the whole window. */
export function genie(
  node: HTMLElement,
  { origin = 'bottom-right' }: { origin?: GenieOrigin } = {},
): TransitionConfig {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return { duration: 0 };

  const { width, height } = node.getBoundingClientRect();
  const style = getComputedStyle(node);
  const base = style.transform === 'none' ? '' : `${style.transform} `;
  const opacity = Number(style.opacity);
  const top = origin === 'top-right';
  const anchor = origin === 'bottom-left' ? 0.08 : 0.92;
  const transformOrigin = `${anchor * 100}% ${top ? 0 : 100}%`;

  return {
    duration: 360,
    easing: cubicOut,
    css: (t) => {
      const remaining = 1 - t;
      const neck = remaining ** 1.6;
      const left = width * anchor * neck;
      const right = width * (1 - (1 - anchor) * neck);
      // Curved sides release from a narrow neck into the surface's full shape.
      const path = top
        ? `M ${left} 0 L ${right} 0 C ${right} ${height * 0.36}, ${width} ${height * 0.55}, ${width} ${height} L 0 ${height} C 0 ${height * 0.55}, ${left} ${height * 0.36}, ${left} 0 Z`
        : `M 0 0 L ${width} 0 C ${width} ${height * 0.45}, ${right} ${height * 0.64}, ${right} ${height} L ${left} ${height} C ${left} ${height * 0.64}, 0 ${height * 0.45}, 0 0 Z`;
      const x = 0.2 + 0.8 * (1 - remaining ** 2);
      const y = 0.035 + 0.965 * t;
      const travel = (top ? -1 : 1) * 28 * remaining ** 2;
      const tilt = (top ? -1 : 1) * 12 * remaining;
      return `
        transform-origin: ${transformOrigin};
        transform: ${base}perspective(900px) translate3d(0, ${travel}px, 0) rotateX(${tilt}deg) scale(${x}, ${y});
        clip-path: path("${path}");
        opacity: ${opacity * Math.min(1, t * 3.6)};
      `;
    },
  };
}
