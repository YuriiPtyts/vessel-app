import { cubicOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

type RevealOrigin = 'bottom-left' | 'bottom-right' | 'top-right';

/** A short, proportional reveal: content keeps its shape as one whole surface. */
export function panelReveal(
  node: HTMLElement,
  { origin = 'bottom-right' }: { origin?: RevealOrigin } = {},
): TransitionConfig {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return { duration: 0 };

  const style = getComputedStyle(node);
  const base = style.transform === 'none' ? '' : `${style.transform} `;
  const top = origin === 'top-right';
  const anchor = origin === 'bottom-left' ? 0 : 1;
  const transformOrigin = `${anchor * 100}% ${top ? 0 : 100}%`;

  return {
    duration: 220,
    easing: cubicOut,
    css: (t) => {
      const remaining = 1 - t;
      const scale = 0.965 + 0.035 * t;
      const travel = (top ? -1 : 1) * 8 * remaining;
      return `
        transform-origin: ${transformOrigin};
        transform: ${base}translate3d(0, ${travel}px, 0) scale(${scale});
        --reveal-content-opacity: ${t};
      `;
    },
  };
}
