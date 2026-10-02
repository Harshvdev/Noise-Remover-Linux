import { writable, derived } from 'svelte/store';

export type Theme = 'dark' | 'light';

function getInitialTheme(): Theme {
  if (typeof window !== 'undefined') {
    const saved = localStorage.getItem('vcleaner_theme');
    if (saved === 'light' || saved === 'dark') {
      return saved;
    }
  }
  return 'light';
}

const initial = getInitialTheme();
export const currentTheme = writable<Theme>(initial);

export function applyTheme(t: Theme) {
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', t);
  }
}

export function setTheme(newTheme: Theme) {
  currentTheme.set(newTheme);
  if (typeof window !== 'undefined') {
    localStorage.setItem('vcleaner_theme', newTheme);
    applyTheme(newTheme);
  }
}

export function toggleTheme() {
  currentTheme.update((t) => {
    const next = t === 'dark' ? 'light' : 'dark';
    if (typeof window !== 'undefined') {
      localStorage.setItem('vcleaner_theme', next);
      applyTheme(next);
    }
    return next;
  });
}

// Canvas & reactive color specifications
export interface ThemeColorSpecs {
  accentHex: string;
  accentRgb: [number, number, number];
  rawWaveRgb: [number, number, number];
  particleAlphaBase: number;
  particleRgbPrefix: string;
}

export const themeColors = derived<typeof currentTheme, ThemeColorSpecs>(
  currentTheme,
  ($t): ThemeColorSpecs => {
    if ($t === 'light') {
      return {
        accentHex: '#FF6600',
        accentRgb: [255, 102, 0],
        rawWaveRgb: [135, 140, 150],
        particleAlphaBase: 0.18,
        particleRgbPrefix: '0, 0, 0, '
      };
    }
    return {
      accentHex: '#C6FF3D',
      accentRgb: [198, 255, 61],
      rawWaveRgb: [215, 215, 230],
      particleAlphaBase: 1.0,
      particleRgbPrefix: '255, 255, 255, '
    };
  }
);
