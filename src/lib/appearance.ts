export const DEFAULT_THEME = { id: 'duskfox', name: 'Duskfox', variant: 'Adaptive glass' };
export const GLASS_TINT_MIN = 50;
export const GLASS_TINT_MAX = 100;
export const ACCENTS = [
  { id: 'rose', name: 'Rose', color: '#ea9a97' },
  { id: 'iris', name: 'Iris', color: '#c4a7e7' },
  { id: 'foam', name: 'Foam', color: '#9ccfd8' },
  { id: 'gold', name: 'Gold', color: '#f6c177' },
] as const;

export interface AppearancePreferences {
  version: 2;
  colorMode: 'light' | 'dark';
  accent: 'rose' | 'iris' | 'foam' | 'gold';
  glassOpacity: number;
  density: 'comfortable' | 'compact';
  sidebarCollapsed: boolean;
  showNowPlaying: boolean;
  reducedTransparency: boolean;
}

export function defaultAppearance(reducedTransparency = false): AppearancePreferences {
  return {
    version: 2,
    colorMode: reducedTransparency ? 'dark' : 'light',
    accent: 'rose',
    glassOpacity: 50,
    density: 'comfortable',
    sidebarCollapsed: false,
    showNowPlaying: true,
    reducedTransparency,
  };
}

export function usesOpaqueMaterial(preferences: AppearancePreferences): boolean {
  return preferences.reducedTransparency || preferences.glassOpacity === GLASS_TINT_MAX;
}

export function usesLightGlass(preferences: AppearancePreferences): boolean {
  return preferences.colorMode === 'light';
}

export function normalizeAppearance(value: unknown, systemReduced = false): AppearancePreferences {
  const defaults = defaultAppearance(systemReduced);
  if (typeof value !== 'object' || value === null) return defaults;
  const data: Record<string, unknown> = Object.fromEntries(Object.entries(value));
  if (data.version !== undefined && data.version !== 1 && data.version !== 2) return defaults;
  const glassOpacity =
    typeof data.glassOpacity === 'number' && Number.isFinite(data.glassOpacity)
      ? data.version === 1 && data.glassOpacity === 56
        ? defaults.glassOpacity
        : Math.round(Math.max(GLASS_TINT_MIN, Math.min(GLASS_TINT_MAX, data.glassOpacity)))
      : defaults.glassOpacity;
  const reducedTransparency =
    typeof data.reducedTransparency === 'boolean'
      ? data.reducedTransparency
      : defaults.reducedTransparency;
  return {
    version: 2,
    // Infer the old beta.7 appearance once, then keep the explicit choice when
    // tint/opaque mode changes. Other saved preferences retain their v2 values.
    colorMode:
      data.colorMode === 'light' || data.colorMode === 'dark'
        ? data.colorMode
        : !reducedTransparency && glassOpacity < 70
          ? 'light'
          : 'dark',
    accent:
      data.accent === 'iris' || data.accent === 'foam' || data.accent === 'gold'
        ? data.accent
        : 'rose',
    glassOpacity,
    density: data.density === 'compact' ? 'compact' : 'comfortable',
    sidebarCollapsed:
      typeof data.sidebarCollapsed === 'boolean'
        ? data.sidebarCollapsed
        : defaults.sidebarCollapsed,
    showNowPlaying:
      typeof data.showNowPlaying === 'boolean' ? data.showNowPlaying : defaults.showNowPlaying,
    reducedTransparency,
  };
}

export function loadAppearance(storage: Storage, systemReduced: boolean): AppearancePreferences {
  try {
    const saved = storage.getItem('vessel:appearance:v2');
    if (saved) {
      const preferences = normalizeAppearance(JSON.parse(saved), systemReduced);
      saveAppearance(storage, preferences);
      return preferences;
    }
    const legacy = storage.getItem('vessel:appearance:v1');
    if (legacy) {
      const migrated = normalizeAppearance(JSON.parse(legacy), systemReduced);
      saveAppearance(storage, migrated);
      return migrated;
    }
    return defaultAppearance(storage.getItem('vessel:opaque') === 'true' || systemReduced);
  } catch {
    return defaultAppearance(systemReduced);
  }
}

export function saveAppearance(storage: Storage, preferences: AppearancePreferences) {
  try {
    storage.setItem('vessel:appearance:v2', JSON.stringify(preferences));
  } catch {
    /* The controls still work for this session when persistence is unavailable. */
  }
}
