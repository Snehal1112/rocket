import { useEffect, useState } from 'react';
import { Toaster as SonnerToaster, type ToasterProps } from 'sonner';

// Tracks the app's `.dark` class rather than the useTheme hook so the toaster
// stays in sync even when the toggle happens from an unrelated component instance.
function useIsDark() {
  const [isDark, setIsDark] = useState(() => document.documentElement.classList.contains('dark'));

  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });
    return () => observer.disconnect();
  }, []);

  return isDark;
}

function Toaster(props: ToasterProps) {
  const isDark = useIsDark();

  return (
    <SonnerToaster
      theme={isDark ? 'dark' : 'light'}
      richColors
      closeButton
      style={
        {
          fontFamily: 'var(--font-sans)',
          '--border-radius': 'var(--radius)',
          '--normal-bg': 'hsl(var(--dropdown-bg))',
          '--normal-text': 'hsl(var(--dropdown-fg))',
          '--normal-border': 'hsl(var(--dropdown-border))',
          '--success-bg': 'hsl(var(--success-soft))',
          '--success-text': 'hsl(var(--success))',
          '--success-border': 'hsl(var(--success) / 0.3)',
          '--warning-bg': 'hsl(var(--warning) / 0.12)',
          '--warning-text': 'hsl(var(--warning))',
          '--warning-border': 'hsl(var(--warning) / 0.3)',
          '--error-bg': 'hsl(var(--destructive-soft))',
          '--error-text': 'hsl(var(--destructive))',
          '--error-border': 'hsl(var(--destructive) / 0.3)',
          '--info-bg': 'hsl(var(--dropdown-bg))',
          '--info-text': 'hsl(var(--notification-info-fg))',
          '--info-border': 'hsl(var(--dropdown-border))',
        } as React.CSSProperties
      }
      {...props}
    />
  );
}

export { Toaster };
