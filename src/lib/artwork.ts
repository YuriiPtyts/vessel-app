/** Upgrade only the provider's known resize syntax; never invent an image for a track. */
export function artworkSource(source: string | null | undefined, size: number): string | undefined {
  if (!source) return undefined;
  if (size < 256) return source;
  try {
    const url = new URL(source);
    if (url.protocol !== 'https:' || !url.hostname.endsWith('.googleusercontent.com'))
      return source;
    const edge = Math.min(512, Math.max(256, Math.round(size)));
    url.pathname = url.pathname.replace(/=w\d+-h\d+(?=-|$)/, `=w${edge}-h${edge}`);
    return url.href;
  } catch {
    return source;
  }
}
