import { type as osType } from '@tauri-apps/plugin-os';
import { useEffect, useState } from 'react';
import { Toaster } from 'sonner';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { ConsolePanel } from '@/components/layout/ConsolePanel';
import { StatusBar } from '@/components/layout/StatusBar';
import { WorkspaceToolbar } from '@/components/layout/WorkspaceToolbar';
import { PaneRenderer } from '@/components/panes/PaneRenderer';
import { SplashScreen } from '@/components/SplashScreen';
import { TitleBar } from '@/components/title-bar';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { restoreUiState, scheduleSaveUiState } from '@/lib/ui-state';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

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
      await loadWorkspaces();
      const uiState = await restoreUiState();
      if (uiState?.activeMode === 'workspace' && uiState.workspaceTabs) {
        const { workspaceId } = uiState.workspaceTabs;
        const ws = useWorkspaceStore.getState().workspaces.find((w) => w.id === workspaceId);
        if (ws) {
          usePaneStore.getState().openWorkspaceTabs(ws.id);
        }
      } else {
        // No saved workspace state or collection mode — show workspace overview.
        const store = useWorkspaceStore.getState();
        const activeWs = store.workspaces.find((w) => w.id === store.activeWorkspaceId);
        if (activeWs) {
          usePaneStore.getState().openWorkspaceTabs(activeWs.id);
        }
      }

      // Load process env vars once at startup — they don't change at runtime.
      void useEnvStore.getState().loadProcessEnvVars();

      // Global env is workspace-scoped; always load it at startup.
      void useEnvStore.getState().fetchGlobalEnv();

      // Load environments for the initial collection, if one is active.
      const initialCollection = usePaneStore.getState().activeCollection;
      if (initialCollection) {
        void useEnvStore.getState().loadEnvironments(initialCollection);
      }
    };
    void init();
  }, [loadWorkspaces]);

  useEffect(() => {
    const unsub = usePaneStore.subscribe(scheduleSaveUiState);
    return unsub;
  }, []);

  useEffect(() => {
    if (osType() === 'linux') {
      document.documentElement.classList.add('linux');
    }
  }, []);

  return (
    <div className='h-full flex flex-col overflow-hidden bg-background text-sm'>
      <TitleBar />
      <div className='flex-1 flex overflow-hidden'>
        {!sidebarCollapsed && (
          <>
            <div
              style={{ '--sidebar-w': `${sidebarWidth}px` } as React.CSSProperties}
              className='w-(--sidebar-w) shrink-0'
            >
              <ErrorBoundary>
                <CollectionsSidebar />
              </ErrorBoundary>
            </div>
            <hr
              className='h-full w-0.5 shrink-0 cursor-col-resize bg-border/50 transition-colors hover:bg-primary/50 border-0'
              aria-orientation='vertical'
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
        <main className='flex-1 flex flex-col min-w-0 overflow-hidden'>
          <WorkspaceToolbar />
          <div className='flex-1 min-h-0 overflow-hidden'>
            <ErrorBoundary>
              <PaneRenderer node={root} />
            </ErrorBoundary>
          </div>
        </main>
      </div>
      <ConsolePanel
        isOpen={isConsoleOpen}
        height={consoleHeight}
        onHeightChange={setConsoleHeight}
      />
      <StatusBar
        isConsoleOpen={isConsoleOpen}
        onConsoleToggle={() => setIsConsoleOpen((o) => !o)}
      />
      {showSplash && <SplashScreen onComplete={() => setShowSplash(false)} />}
      <Toaster position='bottom-right' richColors closeButton />
    </div>
  );
}

export default App;
