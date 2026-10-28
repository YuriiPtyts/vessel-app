// Native modal semantics provide focus trapping, inert surroundings, and Escape.
export function openModal(node: HTMLDialogElement, onclose: () => void) {
  const previous = document.activeElement;
  node.showModal();
  node.querySelector<HTMLElement>('[data-initial-focus]')?.focus();
  const cancel = (event: Event) => {
    event.preventDefault();
    onclose();
  };
  let outside = false;
  const isOutside = (event: PointerEvent) => {
    const box = node.getBoundingClientRect();
    return (
      event.target === node &&
      (event.clientX < box.left ||
        event.clientX > box.right ||
        event.clientY < box.top ||
        event.clientY > box.bottom)
    );
  };
  const down = (event: PointerEvent) => {
    outside = isOutside(event);
  };
  const up = (event: PointerEvent) => {
    if (outside && isOutside(event)) onclose();
    outside = false;
  };
  node.addEventListener('cancel', cancel);
  node.addEventListener('pointerdown', down);
  node.addEventListener('pointerup', up);
  return {
    destroy() {
      node.removeEventListener('cancel', cancel);
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointerup', up);
      node.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    },
  };
}
