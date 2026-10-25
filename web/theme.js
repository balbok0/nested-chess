export const THEMES = ['system', 'classic', 'classic-dark', 'light', 'dark'];

export function resolveTheme(theme, prefersDark) {
  return theme === 'system' ? prefersDark ? 'classic-dark' : 'classic' : theme;
}
