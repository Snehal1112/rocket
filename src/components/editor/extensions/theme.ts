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
    // line-height equal to the inner height of the h-9 wrapper (36px − 2px border)
    // naturally centers the single text line and the placeholder widget.
    overflow: 'hidden',
    lineHeight: '34px',
    fontFamily: 'inherit',
  },
  '.cm-content': {
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
    background: 'rgba(234, 179, 8, 0.15)',
    color: 'rgb(180, 83, 9)', // amber-700
  },
  '.cm-var-collection': {
    background: 'hsl(var(--muted-foreground) / 0.15)',
    color: 'hsl(var(--muted-foreground))',
  },
  '.cm-var-global': {
    background: 'rgba(59, 130, 246, 0.15)',
    color: 'rgb(29, 78, 216)', // blue-700
  },
  '.cm-var-folder': {
    background: 'rgba(20, 184, 166, 0.15)',
    color: 'rgb(15, 118, 110)', // teal-700
  },
  '.cm-var-request, .cm-var-runtime': {
    background: 'rgba(34, 197, 94, 0.15)',
    color: 'rgb(21, 128, 61)', // green-700
  },
  '.cm-var-process': {
    background: 'rgba(100, 116, 139, 0.15)',
    color: 'rgb(51, 65, 85)', // slate-700
  },
  '.cm-var-dynamic': {
    background: 'rgba(6, 182, 212, 0.15)',
    color: 'rgb(14, 116, 144)', // cyan-700
  },
  '.cm-var-unresolved': {
    background: 'hsl(var(--destructive) / 0.15)',
    color: 'hsl(var(--destructive))',
  },
  // Path param token styles (URL bar only).
  '.cm-pathparam': {
    borderRadius: '3px',
    padding: '0 3px',
    background: 'rgba(139, 92, 246, 0.15)',
    color: 'rgb(109, 40, 217)', // violet-700
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
    background: 'rgba(168, 85, 247, 0.1)',
    color: 'rgb(126, 34, 206)', // purple-700
  },
  // Secret mask.
  '.cm-secret-mask': {
    letterSpacing: '1px',
  },

  // Autocomplete dropdown — matches shadcn DropdownMenuContent tokens exactly.
  '.cm-tooltip.cm-tooltip-autocomplete': {
    border: '1px solid hsl(var(--border) / 0.6)',
    borderRadius: 'calc(var(--radius) - 4px)',
    boxShadow: '0 2px 8px rgba(0,0,0,0.16)',
    background: 'hsl(var(--card) / 0.5)',
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
    border: '1px solid hsl(var(--border) / 0.6)',
    borderRadius: 'calc(var(--radius) - 4px)',
    boxShadow: '0 2px 8px rgba(0,0,0,0.16)',
    background: 'hsl(var(--card) / 0.5)',
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
      color: 'rgb(253, 224, 71)', // amber-300
    },
    '.cm-var-global': {
      color: 'rgb(147, 197, 253)', // blue-300
    },
    '.cm-var-folder': {
      color: 'rgb(94, 234, 212)', // teal-300
    },
    '.cm-var-request, .cm-var-runtime': {
      color: 'rgb(134, 239, 172)', // green-300
    },
    '.cm-var-dynamic': {
      color: 'rgb(103, 232, 249)', // cyan-300
    },
    '.cm-var-process': {
      color: 'rgb(203, 213, 225)', // slate-300
    },
    '.cm-pathparam': {
      color: 'rgb(196, 181, 253)', // violet-300
    },
    '.cm-querykey': {
      color: 'rgb(216, 180, 254)', // purple-300
    },
  },
  { dark: true },
);
