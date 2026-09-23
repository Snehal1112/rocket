import { EditorView } from '@codemirror/view';

/**
 * CM6 theme that makes the EditorView look like a shadcn/ui Input component.
 * Uses CSS custom properties so it automatically adapts to light/dark mode.
 *
 * The wrapper div (in SingleLineEditor.tsx) provides:
 *   h-9 rounded-md border border-input bg-background dark:bg-input/30 shadow-xs
 *   focus-within:ring-[3px] focus-within:border-ring
 *
 * This theme handles the inner editor content styling only.
 */
export const rocketTheme = EditorView.theme({
  '&': {
    fontSize: '12px',
    fontFamily: 'var(--font-mono, ui-monospace, monospace)',
    height: '100%',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '.cm-scroller': {
    // CodeMirror's own base theme makes .cm-scroller `display: flex !important`
    // with `align-items: flex-start !important` (for gutter layout). Overriding
    // just the alignment to `center` (also !important, to win against CM's own
    // rule) centers the single text line and placeholder widget vertically
    // regardless of the wrapper's height — unlike a fixed line-height, this
    // keeps working when a consumer overrides the default h-9 (e.g. h-7 rows).
    overflow: 'hidden',
    alignItems: 'center !important',
    lineHeight: '1.4',
    fontFamily: 'inherit',
  },
  '.cm-content': {
    // CM's base theme sets .cm-content { min-height: 100% } so a normal
    // multi-line editor's click-to-focus area fills the scroller even when
    // the document is short. For a single-line field that forces .cm-content
    // (the sole flex item in .cm-scroller) to always fill the full height,
    // leaving align-items no slack to center into. Overriding it to 'auto'
    // lets .cm-content size to its own line + padding, which is what
    // align-items: center above actually needs to have any effect.
    minHeight: 'auto',
    padding: '0',
    caretColor: 'hsl(var(--foreground))',
  },
  '.cm-line': {
    padding: '0 12px', // px-3
  },
  '.cm-cursor': {
    borderLeftColor: 'hsl(var(--foreground))',
  },
  '.cm-placeholder': {
    color: 'hsl(var(--muted-foreground))',
    fontStyle: 'normal',
    // Override CM6 base theme which sets vertical-align: top.
    verticalAlign: 'middle',
  },
  '.cm-selectionBackground': {
    background: 'hsl(var(--primary) / 0.2) !important',
  },
  '&.cm-focused .cm-selectionBackground': {
    background: 'hsl(var(--primary) / 0.3) !important',
  },
  // Variable token base styles. Horizontal padding only so the token
  // does not extend beyond the line box height and shift the baseline.
  '.cm-var': {
    borderRadius: '3px',
    padding: '0 3px',
  },
  '.cm-var-environment': {
    background: 'hsl(var(--warning) / 0.15)',
    color: 'hsl(var(--warning))',
  },
  '.cm-var-collection': {
    background: 'hsl(var(--muted-foreground) / 0.12)',
    color: 'hsl(var(--muted-foreground))',
  },
  '.cm-var-vault': {
    background: 'rgb(244 63 94 / 0.12)',
    color: 'rgb(225 29 72)',
  },
  '.cm-var-global': {
    background: 'hsl(var(--primary) / 0.12)',
    color: 'hsl(var(--primary))',
  },
  '.cm-var-folder': {
    background: 'hsl(var(--git-added) / 0.12)',
    color: 'hsl(var(--git-added))',
  },
  '.cm-var-request, .cm-var-runtime': {
    background: 'hsl(var(--git-added) / 0.12)',
    color: 'hsl(var(--git-added))',
  },
  '.cm-var-process': {
    background: 'hsl(var(--muted-foreground) / 0.12)',
    color: 'hsl(var(--muted-foreground))',
  },
  '.cm-var-dynamic': {
    background: 'hsl(var(--primary) / 0.10)',
    color: 'hsl(var(--primary))',
  },
  '.cm-var-unresolved': {
    background: 'hsl(var(--destructive) / 0.15)',
    color: 'hsl(var(--destructive))',
  },
  // Path param token styles (URL bar only).
  '.cm-pathparam': {
    borderRadius: '3px',
    padding: '0 3px',
    background: 'hsl(var(--chart-4) / 0.12)',
    color: 'hsl(var(--chart-4))',
  },
  '.cm-pathparam-unresolved': {
    borderRadius: '3px',
    padding: '0 3px',
    background: 'hsl(var(--destructive) / 0.15)',
    color: 'hsl(var(--destructive))',
  },
  // Query key styles.
  '.cm-querykey': {
    borderRadius: '3px',
    padding: '0 3px',
    background: 'hsl(var(--chart-4) / 0.12)',
    color: 'hsl(var(--chart-4))',
  },
  // Secret mask.
  '.cm-secret-mask': {
    letterSpacing: '1px',
  },

  // Autocomplete dropdown — matches shadcn DropdownMenuContent tokens exactly.
  '.cm-tooltip.cm-tooltip-autocomplete': {
    border: '1px solid hsl(var(--dropdown-border))',
    borderRadius: 'calc(var(--radius) - 4px)',
    boxShadow: '0 2px 8px rgba(0,0,0,0.16)',
    background: 'hsl(var(--dropdown-bg) / 0.50)',
    backdropFilter: 'blur(8px)',
    WebkitBackdropFilter: 'blur(8px)',
    overflow: 'hidden',
  },
  '.cm-tooltip.cm-tooltip-autocomplete > ul': {
    fontFamily: 'var(--font-mono, ui-monospace, monospace)',
    fontSize: '12px',
    background: 'transparent',
    maxHeight: '200px',
    minWidth: '200px',
    padding: '4px',
  },
  '.cm-tooltip.cm-tooltip-autocomplete > ul > li': {
    borderRadius: '0',
    padding: '4px 12px',
    lineHeight: '1.5',
    color: 'hsl(var(--popover-foreground))',
    display: 'flex',
    alignItems: 'center',
  },
  '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    background: 'hsl(var(--accent))',
    color: 'hsl(var(--accent-foreground))',
  },
  '.cm-completionLabel': {
    flex: '1',
  },
  '.cm-completionDetail': {
    marginLeft: '8px',
    fontSize: '10px',
    fontStyle: 'normal',
    color: 'hsl(var(--muted-foreground))',
    opacity: '1',
  },
  // Info panel (resolved value tooltip).
  '.cm-tooltip.cm-completionInfo': {
    border: '1px solid hsl(var(--dropdown-border))',
    borderRadius: 'calc(var(--radius) - 4px)',
    boxShadow: '0 2px 8px rgba(0,0,0,0.16)',
    background: 'hsl(var(--dropdown-bg) / 0.50)',
    backdropFilter: 'blur(8px)',
    WebkitBackdropFilter: 'blur(8px)',
    color: 'hsl(var(--muted-foreground))',
    fontSize: '11px',
    fontFamily: 'var(--font-mono, ui-monospace, monospace)',
    padding: '4px 8px',
    maxWidth: '300px',
  },
});

/**
 * Base theme override to strip CM6's hardcoded grey border and background from
 * ALL .cm-tooltip elements. Must use EditorView.baseTheme() (not .theme()) because
 * CM6's own base theme uses &light/.cm-tooltip selectors that .theme() cannot override.
 * The variable popover React card provides its own border; the autocomplete dropdown
 * re-applies its border via .cm-tooltip-autocomplete below.
 */
export const rocketTooltipBase = EditorView.baseTheme({
  '&light .cm-tooltip': {
    border: 'none',
    background: 'transparent',
  },
  '&dark .cm-tooltip': {
    border: 'none',
    background: 'transparent',
  },
});

/**
 * Dark mode overrides. Uses the same CSS custom properties so most colors
 * adapt automatically, but text colors on colored backgrounds need lighter
 * variants in dark mode for contrast.
 */
export const rocketThemeDark = EditorView.theme(
  {
    '.cm-tooltip.cm-tooltip-autocomplete': {
      boxShadow: '0 2px 8px rgba(0,0,0,0.4)',
    },
    '.cm-tooltip.cm-completionInfo': {
      boxShadow: '0 2px 8px rgba(0,0,0,0.4)',
    },
    '.cm-var-environment': {
      background: 'hsl(var(--warning) / 0.15)',
      color: '#e5ba7d', // VSCode 2026 Dark: gitDecoration.modifiedResourceForeground
    },
    '.cm-var-collection': {
      background: 'hsl(var(--muted-foreground) / 0.12)',
      color: '#8c8c8c', // VSCode 2026 Dark: descriptionForeground
    },
    '.cm-var-vault': {
      background: 'rgb(244 63 94 / 0.15)',
      color: '#f14c71',
    },
    '.cm-var-global': {
      background: 'hsl(var(--primary) / 0.15)',
      color: '#3994bc', // VSCode 2026 Dark: focusBorder / ring
    },
    '.cm-var-folder': {
      background: 'hsl(var(--git-added) / 0.12)',
      color: '#73c991', // VSCode 2026 Dark: gitDecoration.addedResourceForeground
    },
    '.cm-var-request, .cm-var-runtime': {
      background: 'hsl(var(--git-added) / 0.12)',
      color: '#73c991', // VSCode 2026 Dark: gitDecoration.addedResourceForeground
    },
    '.cm-var-dynamic': {
      background: 'hsl(var(--primary) / 0.12)',
      color: '#3994bc', // VSCode 2026 Dark: primary / ring
    },
    '.cm-var-process': {
      background: 'hsl(var(--muted-foreground) / 0.12)',
      color: '#8c8c8c', // VSCode 2026 Dark: descriptionForeground
    },
    '.cm-pathparam': {
      background: 'hsl(var(--chart-4) / 0.12)',
      color: '#ad80d7', // VSCode 2026 Dark: charts.purple approx
    },
    '.cm-querykey': {
      background: 'hsl(var(--chart-4) / 0.10)',
      color: '#ad80d7', // VSCode 2026 Dark: charts.purple approx
    },
  },
  { dark: true },
);
