import type { Monaco } from '@monaco-editor/react';
import { loader } from '@monaco-editor/react';
import { useEffect, useState } from 'react';
import { defineMonacoThemes } from './monaco-config';

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

  // Re-define themes with current CSS variable values, then apply.
  // Themes must be re-registered on each toggle because their colors are
  // computed from CSS custom properties that change with the app theme.
  useEffect(() => {
    const name = isDark ? 'rocket-dark' : 'rocket-light';
    loader.init().then((monaco) => {
      defineMonacoThemes(monaco);
      monaco.editor.setTheme(name);
    });
  }, [isDark]);

  const themeName = isDark ? 'rocket-dark' : 'rocket-light';

  // Themes are defined in monaco-setup.ts before any editor mounts.
  // This is a no-op safety net for editors that mount before monaco-setup runs.
  function defineThemes(monaco: Monaco) {
    defineMonacoThemes(monaco);
  }

  return { themeName, defineThemes, isDark };
}
