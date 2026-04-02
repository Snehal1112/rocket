import { type as osType } from '@tauri-apps/plugin-os';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { TitleBar } from '@/components/title-bar';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { ConsolePanel } from '@/components/layout/ConsolePanel';
import { StatusBar } from '@/components/layout/StatusBar';
import { WorkspaceToolbar } from '@/components/layout/WorkspaceToolbar';
import { PaneRenderer } from '@/components/panes/PaneRenderer';
import { SplashScreen } from '@/components/SplashScreen';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { useEnvStore } from '@/stores/env-store';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { restoreUiState, scheduleSaveUiState } from '@/lib/ui-state';
import { useState, useEffect } from 'react';

function App() {
  const root = usePaneStore((s) => s.root);
  const [showSplash, setShowSplash] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(280);
  const [sidebarCollapsed] = useState(false);
  const [isConsoleOpen, setIsConsoleOpen] = useState(false);
  const [consoleHeight, setConsoleHeight] = useState(280);
  useKeyboardShortcuts();

  const loadWorkspaces = useWorkspaceStore((s) => s.loadWorkspaces);
  useEffect(() => {
    const init = async () => {
      await loadWorkspaces()
      const uiState = await restoreUiState()
      if (uiState?.activeMode === 'workspace' && uiState.workspaceTabs) {
        const { workspaceId } = uiState.workspaceTabs
        const ws = useWorkspaceStore.getState().workspaces.find((w) => w.id === workspaceId)
        if (ws) {
          usePaneStore.getState().openWorkspaceTabs(ws.id)
        }
      }
      // Task 9: First-launch fallback
      if (!uiState) {
        const store = useWorkspaceStore.getState()
        const activeWs = store.workspaces.find((w) => w.id === store.activeWorkspaceId)
        if (activeWs) {
          usePaneStore.getState().openWorkspaceTabs(activeWs.id)
        }
      }

      // Load process env vars once at startup — they don't change at runtime.
      void useEnvStore.getState().loadProcessEnvVars();

      // Load environments for the initial collection, if one is active.
      const initialCollection = usePaneStore.getState().activeCollection;
      if (initialCollection) {
        void useEnvStore.getState().loadEnvironments(initialCollection);
        void useEnvStore.getState().fetchGlobalEnv();
      }
    }
    void init()
  }, [loadWorkspaces]);

  useEffect(() => {
    const unsub = usePaneStore.subscribe(scheduleSaveUiState)
    return unsub
  }, []);

  useEffect(() => {
    if (osType() === 'linux') {
      document.documentElement.classList.add('linux');
    }
  }, []);

  // Preload Monaco in the background after the app shell renders.
  // requestIdleCallback is Chrome-only; setTimeout works in all WebViews.
  useEffect(() => {
    const id = setTimeout(() => { void import('@/components/editor/MonacoWrapper'); }, 200);
    return () => clearTimeout(id);
  }, []);

  return (
    <div className="h-full flex flex-col overflow-hidden bg-background text-sm">
      <TitleBar />
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
          <WorkspaceToolbar />
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
