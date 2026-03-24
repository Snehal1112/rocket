import { Header } from '@/components/layout/Header';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { StatusBar } from '@/components/layout/StatusBar';
import { PaneRenderer } from '@/components/panes/PaneRenderer';
import { usePaneStore } from '@/stores/pane-store';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { useState } from 'react';

function App() {
  const root = usePaneStore((s) => s.root);
  const [sidebarWidth, setSidebarWidth] = useState(280);
  const [sidebarCollapsed] = useState(false);
  useKeyboardShortcuts();

  return (
    <div className="h-screen flex flex-col bg-gradient-to-br from-background via-background to-accent/25 text-sm">
      <Header />
      <div className="flex-1 flex overflow-hidden">
        {!sidebarCollapsed && (
          <>
            <div style={{ width: `${sidebarWidth}px` }} className="shrink-0">
              <CollectionsSidebar />
            </div>
            <div
              role="separator"
              className="w-1.5 shrink-0 cursor-col-resize bg-border/35 transition-colors hover:bg-primary/35"
              onPointerDown={(e) => {
                e.preventDefault();
                const startX = e.clientX;
                const startWidth = sidebarWidth;
                const onMove = (ev: PointerEvent) => {
                  const newWidth = Math.min(500, Math.max(200, startWidth + ev.clientX - startX));
                  setSidebarWidth(newWidth);
                };
                const onUp = () => {
                  window.removeEventListener('pointermove', onMove);
                  window.removeEventListener('pointerup', onUp);
                };
                window.addEventListener('pointermove', onMove);
                window.addEventListener('pointerup', onUp);
              }}
            />
          </>
        )}
        <main className="flex-1 flex flex-col min-w-0">
          <PaneRenderer node={root} />
        </main>
      </div>
      <StatusBar />
    </div>
  );
}

export default App;
