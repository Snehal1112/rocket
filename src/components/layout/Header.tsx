import { Button } from '@/components/ui/button';
import { Sun, Moon } from 'lucide-react';
import { useTheme } from '@/hooks/useTheme';

// Top header bar with logo, title, and theme toggle.
export function Header() {
  const { isDark, toggleTheme } = useTheme();

  return (
    <header className="h-14 border-b border-border/70 flex items-center px-4 bg-card/70 backdrop-blur-md shrink-0">
      <div className="flex items-center gap-2.5">
        <img src="/rocket.png" alt="Rocket API" className="w-7 h-7 object-contain" />
        <div className="leading-tight">
          <p className="font-semibold tracking-tight text-foreground">Rocket</p>
          <p className="text-xs text-muted-foreground">API Workspace</p>
        </div>
      </div>
      <div className="flex-1" />
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="icon"
          onClick={toggleTheme}
          className="h-8 w-8 rounded-full border-border/70 bg-card/70 backdrop-blur"
          title={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {isDark ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
        </Button>
      </div>
    </header>
  );
}
