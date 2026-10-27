export interface VolumeControl {
  (value: number): Promise<void>;
  flush(): Promise<void>;
}

/** Live volume has its own lane: one native write in flight, with the newest value next. */
export function createVolumeControl(
  apply: (value: number) => Promise<void>,
  onError: (error: unknown) => void,
): VolumeControl {
  let pending: number | null = null;
  let writing: Promise<void> | null = null;

  async function flush() {
    while (pending !== null) {
      const value = pending;
      pending = null;
      try {
        await apply(value);
      } catch (error) {
        onError(error);
      }
    }
  }

  const control: VolumeControl = (value) => {
    if (!Number.isFinite(value)) return Promise.resolve();
    pending = Math.max(0, Math.min(100, value));
    writing ??= flush().finally(() => {
      writing = null;
    });
    return writing;
  };
  control.flush = () => writing ?? Promise.resolve();
  return control;
}
