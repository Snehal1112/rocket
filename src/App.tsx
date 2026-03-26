import { ErrorBoundary } from '@/components/ErrorBoundary';
import { Header } from '@/components/layout/Header';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { ConsolePanel } from '@/components/layout/ConsolePanel';
import { StatusBar } from '@/components/layout/StatusBar';
import { PaneRenderer } from '@/components/panes/PaneRenderer';
import { SplashScreen } from '@/components/SplashScreen';
import { usePaneStore } from '@/stores/pane-store';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { useState } from 'react';

function App() {
  const root = usePaneStore((s) => s.root);
  const [showSplash, setShowSplash] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(280);
  const [sidebarCollapsed] = useState(false);
  const [isConsoleOpen, setIsConsoleOpen] = useState(false);
  const [consoleHeight, setConsoleHeight] = useState(280);
  useKeyboardShortcuts();

  return (
    <div className="h-screen flex flex-col overflow-hidden bg-gradient-to-br from-background via-background to-accent/25 text-sm">
      <Header />
      <div className="flex-1 flex overflow-hidden">
        {!sidebarCollapsed && (
          <>
            <div style={{ '--sidebar-w': `${sidebarWidth}px` } as React.CSSProperties} className="w-[var(--sidebar-w)] shrink-0">
              <ErrorBoundary>
                <CollectionsSidebar />
              </ErrorBoundary>
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
          <ErrorBoundary>
            <PaneRenderer node={root} />
          </ErrorBoundary>
        </main>
      </div>
      <ConsolePanel isOpen={isConsoleOpen} height={consoleHeight} onHeightChange={setConsoleHeight} />
      <StatusBar isConsoleOpen={isConsoleOpen} onConsoleToggle={() => setIsConsoleOpen((o) => !o)} />
      {showSplash && <SplashScreen onComplete={() => setShowSplash(false)} />}
    </div>
  );
}

export default App;
