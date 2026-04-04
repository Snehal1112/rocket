import type { Monaco } from '@monaco-editor/react';
import { loader } from '@monaco-editor/react';
import { useEffect, useState } from 'react';

export function useMonacoTheme() {
  const [isDark, setIsDark] = useState(() => document.documentElement.classList.contains('dark'));

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mq.addEventListener('change', handler);

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

  // Apply theme change globally so all editors update in real time.
  useEffect(() => {
    const name = isDark ? 'rocket-dark' : 'rocket-light';
    loader.init().then((monaco) => {
      monaco.editor.setTheme(name);
    });
  }, [isDark]);

  const themeName = isDark ? 'rocket-dark' : 'rocket-light';

  // Themes are defined in main.tsx before any editor mounts.
  // This is a no-op safety net for editors that mount before main.tsx runs.
  function defineThemes(monaco: Monaco) {
    monaco.editor.defineTheme('rocket-light', {
      base: 'vs',
      inherit: true,
      rules: [
        { token: 'string', foreground: 'a31515' },
        { token: 'string.key.json', foreground: '0451a5' },
        { token: 'number', foreground: '098658' },
        { token: 'keyword', foreground: '0000ff' },
        { token: 'comment', foreground: '008000' },
        { token: 'type', foreground: '267f99' },
        { token: 'variable', foreground: '001080' },
        { token: 'constant', foreground: '0070c1' },
      ],
      colors: {
        'editor.background': '#ffffff',
        'editor.foreground': '#000000',
        'editor.lineHighlightBackground': '#add6ff26',
        'editorLineNumber.foreground': '#237893',
        'editorLineNumber.activeForeground': '#0b216f',
      },
    });

    monaco.editor.defineTheme('rocket-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'string', foreground: 'ce9178' },
        { token: 'string.key.json', foreground: '9cdcfe' },
        { token: 'number', foreground: 'b5cea8' },
        { token: 'keyword', foreground: '569cd6' },
        { token: 'comment', foreground: '6a9955' },
        { token: 'type', foreground: '4ec9b0' },
        { token: 'variable', foreground: '9cdcfe' },
        { token: 'constant', foreground: '4fc1ff' },
      ],
      colors: {
        'editor.background': '#1f1f1f',
        'editor.foreground': '#d4d4d4',
        'editor.lineHighlightBackground': '#2a2d2e',
        'editorLineNumber.foreground': '#858585',
        'editorLineNumber.activeForeground': '#c6c6c6',
      },
    });
  }

  return { themeName, defineThemes, isDark };
}
