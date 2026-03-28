import { getCurrentWindow } from '@tauri-apps/api/window'
import { Button } from '@/components/ui/button'

export function WindowControls() {
  const win = getCurrentWindow()

  return (
    <div className="flex items-center">
      <Button
        variant="ghost"
        size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground"
        onClick={() => win.minimize()}
        aria-label="Minimize"
      >
        <span className="text-xs">─</span>
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-accent hover:text-accent-foreground"
        onClick={() => win.toggleMaximize()}
        aria-label="Maximize"
      >
        <span className="text-xs">▢</span>
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="h-11 w-12 rounded-none text-muted-foreground hover:bg-destructive hover:text-destructive-foreground"
        onClick={() => win.close()}
        aria-label="Close"
      >
        <span className="text-xs">✕</span>
      </Button>
    </div>
  )
}
