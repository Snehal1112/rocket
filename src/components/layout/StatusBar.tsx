import { Terminal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { EnvironmentSwitcher } from "@/components/layout/EnvironmentSwitcher";
import { useConsoleStore } from "@/stores/console-store";

interface StatusBarProps {
  isConsoleOpen?: boolean;
  onConsoleToggle?: () => void;
}

export function StatusBar({ isConsoleOpen, onConsoleToggle }: StatusBarProps) {
  const entryCount = useConsoleStore((s) => s.entries.length);

  return (
    <div className="h-7 border-t border-border/70 bg-card/85 backdrop-blur-sm px-2 flex items-center gap-1.5 shrink-0">
      <Button
        variant="ghost"
        size="sm"
        className={cn(
          "h-5 px-1.5 text-2xs gap-1",
          isConsoleOpen && "bg-accent",
        )}
        onClick={onConsoleToggle}
        aria-label="Toggle Console"
      >
        <Terminal className="h-3 w-3" />
        Console
        {entryCount > 0 && (
          <span className="text-2xs px-1 rounded-full bg-muted text-muted-foreground">
            {entryCount}
          </span>
        )}
      </Button>
      <div className="ml-auto">
        <EnvironmentSwitcher />
      </div>
    </div>
  );
}
