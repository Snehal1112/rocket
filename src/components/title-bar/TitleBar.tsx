import { type as osType } from "@tauri-apps/plugin-os";
import { Sun, Moon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useTheme } from "@/hooks/useTheme";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";
import { WindowControls } from "./WindowControls";

export function TitleBar() {
  const { isDark, toggleTheme } = useTheme();
  const isMac = osType() === "macos";

  return (
    <div
      className="flex h-11 w-full items-center shrink-0 border-b bg-background"
      data-tauri-drag-region
    >
      {isMac && <div className="w-[72px] shrink-0" data-tauri-drag-region />}

      <div className="flex items-center gap-2 px-3 shrink-0">
        <img src="/rocket.png" alt="Rocket" className="h-4 w-4" />
        <span className="text-sm font-medium">Rocket</span>
      </div>

      <div className="flex-1" data-tauri-drag-region />

      <WorkspaceSwitcher />

      <div className="flex-1" data-tauri-drag-region />

      <Button
        variant="ghost"
        size="icon"
        onClick={toggleTheme}
        className="h-7 w-7 rounded-full mr-2"
        title={isDark ? "Switch to light mode" : "Switch to dark mode"}
      >
        {isDark ? (
          <Sun className="h-3.5 w-3.5" />
        ) : (
          <Moon className="h-3.5 w-3.5" />
        )}
      </Button>

      {!isMac && <WindowControls />}
    </div>
  );
}
