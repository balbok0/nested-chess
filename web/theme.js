// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 nested-chess contributors

export const THEMES = ['system', 'classic', 'classic-dark', 'light', 'dark'];

export function resolveTheme(theme, prefersDark) {
  return theme === 'system' ? prefersDark ? 'classic-dark' : 'classic' : theme;
}
