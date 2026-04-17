import { EditorView } from '@codemirror/view';

/**
 * CM6 theme that makes the EditorView look like a shadcn/ui Input component.
 * Uses CSS custom properties so it automatically adapts to light/dark mode.
 *
 * The wrapper div (in SingleLineEditor.tsx) provides:
 *   h-8 rounded-md border border-input bg-background
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
    // Vertically center the single line inside the h-8 wrapper so that
    // variable badges and plain text share a baseline.
    display: 'flex',
    alignItems: 'center',
    overflow: 'hidden',
    lineHeight: '1',
    fontFamily: 'inherit',
  },
  '.cm-content': {
    padding: '0',
    caretColor: 'hsl(var(--foreground))',
  },
  '.cm-line': {
    // Flex-center so the placeholder widget and inline badges share
    // the same vertical alignment as plain text.
    display: 'flex',
    alignItems: 'center',
    padding: '0 12px', // px-3
  },
  '.cm-cursor': {
    borderLeftColor: 'hsl(var(--foreground))',
  },
  '.cm-placeholder': {
    color: 'hsl(var(--muted-foreground))',
    fontStyle: 'normal',
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
});

/**
 * Dark mode overrides. Uses the same CSS custom properties so most colors
 * adapt automatically, but text colors on colored backgrounds need lighter
 * variants in dark mode for contrast.
 */
export const rocketThemeDark = EditorView.theme(
  {
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
