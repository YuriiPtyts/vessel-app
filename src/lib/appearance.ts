export const DEFAULT_THEME = { id: 'duskfox', name: 'Duskfox', variant: 'Blurred' };
export const ACCENTS = [
  { id: 'rose', name: 'Rose', color: '#ea9a97' },
  { id: 'iris', name: 'Iris', color: '#c4a7e7' },
  { id: 'foam', name: 'Foam', color: '#9ccfd8' },
  { id: 'gold', name: 'Gold', color: '#f6c177' },
] as const;

export interface AppearancePreferences {
  version: 1;
  accent: 'rose' | 'iris' | 'foam' | 'gold';
  glassOpacity: number;
  density: 'comfortable' | 'compact';
  sidebarCollapsed: boolean;
  showNowPlaying: boolean;
  reducedTransparency: boolean;
}

export function defaultAppearance(reducedTransparency = false): AppearancePreferences {
  return {
    version: 1,
    accent: 'rose',
    glassOpacity: 56,
    density: 'comfortable',
    sidebarCollapsed: false,
    showNowPlaying: true,
    reducedTransparency,
  };
}

export function normalizeAppearance(value: unknown, systemReduced = false): AppearancePreferences {
  const defaults = defaultAppearance(systemReduced);
  if (typeof value !== 'object' || value === null) return defaults;
  const data: Record<string, unknown> = Object.fromEntries(Object.entries(value));
  if (data.version !== undefined && data.version !== 1) return defaults;
  return {
    version: 1,
    accent:
      data.accent === 'iris' || data.accent === 'foam' || data.accent === 'gold'
        ? data.accent
        : 'rose',
    glassOpacity:
      typeof data.glassOpacity === 'number' && Number.isFinite(data.glassOpacity)
        ? Math.round(Math.max(30, Math.min(90, data.glassOpacity)))
        : defaults.glassOpacity,
    density: data.density === 'compact' ? 'compact' : 'comfortable',
    sidebarCollapsed:
      typeof data.sidebarCollapsed === 'boolean'
        ? data.sidebarCollapsed
        : defaults.sidebarCollapsed,
    showNowPlaying:
      typeof data.showNowPlaying === 'boolean' ? data.showNowPlaying : defaults.showNowPlaying,
    reducedTransparency:
      typeof data.reducedTransparency === 'boolean'
        ? data.reducedTransparency
        : defaults.reducedTransparency,
  };
}

export function loadAppearance(storage: Storage, systemReduced: boolean): AppearancePreferences {
  try {
    const saved = storage.getItem('vessel:appearance:v1');
    if (saved) return normalizeAppearance(JSON.parse(saved), systemReduced);
    return defaultAppearance(storage.getItem('vessel:opaque') === 'true' || systemReduced);
  } catch {
    return defaultAppearance(systemReduced);
  }
}

export function saveAppearance(storage: Storage, preferences: AppearancePreferences) {
  try {
    storage.setItem('vessel:appearance:v1', JSON.stringify(preferences));
  } catch {
    /* The controls still work for this session when persistence is unavailable. */
  }
}
