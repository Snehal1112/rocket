import { Moon, Sun } from 'lucide-react';
import { useCallback, useState } from 'react';

import { Button } from '@/components/ui/button';

const STORAGE_KEY = 'rocket-theme';

function isDark() {
  return document.documentElement.classList.contains('dark');
}

/**
 * Sun/moon switch for the palette.
 * The first paint is decided by the inline script in `index.html`, which reads
 * the same storage key, so there is no flash of the wrong theme on load.
 */
export function ThemeToggle() {
  // The page renders on the client only, so reading the class here is safe and
  // there is no server markup to disagree with.
  const [dark, setDark] = useState(isDark);

  const toggle = useCallback(() => {
    const next = !isDark();
    document.documentElement.classList.toggle('dark', next);
    try {
      localStorage.setItem(STORAGE_KEY, next ? 'dark' : 'light');
    } catch {
      // Storage can be blocked. The toggle still works for this visit.
    }
    setDark(next);
  }, []);

  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={toggle}
      aria-label={dark ? 'Switch to the light theme' : 'Switch to the dark theme'}
      aria-pressed={dark}
      title={dark ? 'Light theme' : 'Dark theme'}
    >
      {dark ? <Sun /> : <Moon />}
    </Button>
  );
}
