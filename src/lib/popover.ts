interface Options {
  onDismiss: () => void;
  trigger: string;
}
const opened = 'vessel-popover-opened';

/** Dismiss without swallowing the outside click, and keep trigger toggles intact. */
export function dismissOnOutside(node: HTMLElement, options: Options) {
  let current = options;
  function pointer(event: PointerEvent) {
    const path = event.composedPath();
    if (
      path.includes(node) ||
      path.some((target) => target instanceof Element && target.matches(current.trigger))
    )
      return;
    current.onDismiss();
  }
  function keyboard(event: KeyboardEvent) {
    if (event.key === 'Escape') current.onDismiss();
  }
  function another(event: Event) {
    if (event instanceof CustomEvent && event.detail !== node) current.onDismiss();
  }
  document.addEventListener('pointerdown', pointer, true);
  document.addEventListener('keydown', keyboard, true);
  document.addEventListener(opened, another);
  document.dispatchEvent(new CustomEvent(opened, { detail: node }));
  return {
    update(options: Options) {
      current = options;
    },
    destroy() {
      document.removeEventListener('pointerdown', pointer, true);
      document.removeEventListener('keydown', keyboard, true);
      document.removeEventListener(opened, another);
    },
  };
}
