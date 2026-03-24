import { useEffect, useState } from 'react';
import type { Monaco } from '@monaco-editor/react';

export function useMonacoTheme() {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains('dark'),
  );

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mq.addEventListener('change', handler);

    // Also observe the document class for manual dark mode toggle.
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });

    return () => {
      mq.removeEventListener('change', handler);
      observer.disconnect();
    };
  }, []);

  const themeName = isDark ? 'rocket-dark' : 'rocket-light';

  function defineThemes(monaco: Monaco) {
    monaco.editor.defineTheme('rocket-light', {
      base: 'vs',
      inherit: true,
      rules: [
        { token: 'string', foreground: '16a34a' },   // green-600
        { token: 'number', foreground: 'd97706' },   // amber-600
        { token: 'keyword', foreground: '7c3aed' },  // purple-600
        { token: 'comment', foreground: '9ca3af' },  // gray-400
        { token: 'type', foreground: '2563eb' },     // blue-600
      ],
      colors: {
        'editor.background': '#f5f8fc',
        'editor.foreground': '#1a1f36',
        'editor.lineHighlightBackground': '#eef2f9',
        'editorLineNumber.foreground': '#9ca3af',
      },
    });

    monaco.editor.defineTheme('rocket-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'string', foreground: '4ade80' },   // green-400
        { token: 'number', foreground: 'fbbf24' },   // amber-300
        { token: 'keyword', foreground: 'a78bfa' },  // purple-400
        { token: 'comment', foreground: '6b7280' },  // gray-500
        { token: 'type', foreground: '60a5fa' },     // blue-400
      ],
      colors: {
        'editor.background': '#1f1f1f',
        'editor.foreground': '#dedede',
        'editor.lineHighlightBackground': '#242424',
        'editorLineNumber.foreground': '#666666',
      },
    });
  }

  return { themeName, defineThemes, isDark };
}
