import { EditorView } from '@codemirror/view';

export const multiLineTheme = EditorView.theme({
  '&': {
    fontSize: '13px',
    fontFamily: "var(--font-mono, 'JetBrains Mono', monospace)",
    height: '100%',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '.cm-scroller': {
    overflow: 'auto',
    fontFamily: 'inherit',
  },
  '.cm-content': {
    padding: '8px 0',
  },
  '.cm-gutters': {
    borderRight: '1px solid hsl(var(--border))',
    background: 'hsl(var(--background))',
  },
  '.cm-lineNumbers .cm-gutterElement': {
    padding: '0 8px 0 16px',
    minWidth: '40px',
    color: 'hsl(var(--muted-foreground))',
  },
  '.cm-activeLineGutter': {
    background: 'transparent',
    color: 'hsl(var(--foreground))',
  },
  '.cm-activeLine': {
    background: 'hsl(var(--primary) / 0.05)',
  },
  '.cm-foldGutter .cm-gutterElement': {
    padding: '0 4px',
  },
  '.cm-selectionBackground': {
    background: 'hsl(var(--primary) / 0.2) !important',
  },
  '&.cm-focused .cm-selectionBackground': {
    background: 'hsl(var(--primary) / 0.3) !important',
  },
  '.cm-cursor': {
    borderLeftColor: 'hsl(var(--foreground))',
  },
  '.cm-var-hover': {
    padding: '6px 10px',
    fontSize: '12px',
    lineHeight: '1.5',
    fontFamily: 'var(--font-sans, system-ui, sans-serif)',
    color: 'hsl(var(--foreground))',
    background: 'hsl(var(--popover))',
    border: '1px solid hsl(var(--border))',
    borderRadius: '6px',
    boxShadow: '0 2px 8px rgba(0,0,0,0.1)',
    maxWidth: '300px',
  },
  '.cm-var-hover-name': {
    fontFamily: 'var(--font-mono, monospace)',
    fontWeight: '600',
    marginBottom: '2px',
  },
  '.cm-var-hover-meta': {
    color: 'hsl(var(--muted-foreground))',
    fontSize: '11px',
  },
});
