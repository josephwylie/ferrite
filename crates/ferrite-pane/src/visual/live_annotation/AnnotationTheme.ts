// Adapted from T3 Code 4da2bc520c38cd6b36014d2888774168fbf4c77f. MIT, see LICENSE.
import type { DesktopPreviewAnnotationTheme } from "./types.ts";

// Host-owned Code dark theme: packages/shared/src/themePalettes.ts,
// T3_CODE_DARK_THEME_COLORS, mapped by apps/web/src/browser/annotationTheme.ts.
// The standard web theme overrides canvas to black in themePalette.ts;
// radius and local font stacks follow apps/web/src/index.css. These are resolved
// values, never variables or computed styles inherited from the inspected page.
export const annotationTheme: Readonly<DesktopPreviewAnnotationTheme> = Object.freeze({
  colorScheme: "dark",
  radius: "0.5rem",
  background: "#000000",
  foreground: "#f5f5f5",
  popover: "#111111",
  popoverForeground: "#f5f5f5",
  primary: "#346bf1",
  primaryForeground: "#ffffff",
  muted: "#111111",
  mutedForeground: "#818181",
  accent: "#141414",
  accentForeground: "#f5f5f5",
  border: "#191919",
  input: "#1e1e1e",
  ring: "#346bf1",
  fontSans: '-apple-system, BlinkMacSystemFont, "Inter Variable", "Segoe UI", system-ui, sans-serif',
  fontMono: 'ui-monospace, "SF Mono", "SFMono-Regular", Menlo, Consolas, "Liberation Mono", monospace',
});

export function applyAnnotationTheme(host: HTMLElement, theme: Readonly<DesktopPreviewAnnotationTheme>): void {
  host.style.colorScheme = theme.colorScheme;
  const variables = {
    "--t3-radius": theme.radius,
    "--t3-background": theme.background,
    "--t3-foreground": theme.foreground,
    "--t3-popover": theme.popover,
    "--t3-popover-foreground": theme.popoverForeground,
    "--t3-primary": theme.primary,
    "--t3-primary-foreground": theme.primaryForeground,
    "--t3-muted": theme.muted,
    "--t3-muted-foreground": theme.mutedForeground,
    "--t3-accent": theme.accent,
    "--t3-accent-foreground": theme.accentForeground,
    "--t3-border": theme.border,
    "--t3-input": theme.input,
    "--t3-ring": theme.ring,
    "--t3-font-sans": theme.fontSans,
    "--t3-font-mono": theme.fontMono,
  };
  for (const [name, value] of Object.entries(variables)) host.style.setProperty(name, value);
}
