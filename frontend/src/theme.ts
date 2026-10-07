import { webDarkTheme, type Theme } from '@fluentui/react-components';

/** Fluent's standard dark theme. Only typography is adapted: Windows 11's
 * Segoe UI Variable, and sizes that follow the Interface size preference
 * (--ui-font/--ui-note) so Large mode scales every Fluent control. Colors,
 * radii, spacing and shadows are Fluent's own tokens. */
export const localImageTheme: Theme = {
  ...webDarkTheme,
  fontFamilyBase: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
  fontFamilyNumeric: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
  fontSizeBase100: 'var(--ui-note)',
  fontSizeBase200: 'var(--ui-note)',
  fontSizeBase300: 'var(--ui-font)',
  fontSizeBase400: 'calc(var(--ui-font) * 1.142857)',
  fontSizeBase500: 'calc(var(--ui-font) * 1.428571)',
  fontSizeBase600: 'calc(var(--ui-font) * 1.714286)',
  fontSizeHero700: 'calc(var(--ui-font) * 2)',
  fontSizeHero800: 'calc(var(--ui-font) * 2.285714)',
  fontSizeHero900: 'calc(var(--ui-font) * 2.857143)',
  fontSizeHero1000: 'calc(var(--ui-font) * 4.857143)',
  lineHeightBase100: 'calc(var(--ui-note) * 1.4)',
  lineHeightBase200: 'calc(var(--ui-note) * 1.4)',
  lineHeightBase300: 'calc(var(--ui-font) * 1.4)',
  lineHeightBase400: 'calc(var(--ui-font) * 1.571429)',
  lineHeightBase500: 'calc(var(--ui-font) * 2)',
  lineHeightBase600: 'calc(var(--ui-font) * 2.285714)',
  lineHeightHero700: 'calc(var(--ui-font) * 2.571429)',
  lineHeightHero800: 'calc(var(--ui-font) * 2.857143)',
  lineHeightHero900: 'calc(var(--ui-font) * 3.714286)',
  lineHeightHero1000: 'calc(var(--ui-font) * 6.571429)',
};
