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
import { useLayoutStore } from '@/stores/layout-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { CollectionTab } from '@/types/pane-types';

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
      if (uiState?.layoutDirection) {
        useLayoutStore.getState().setRequestLayout(uiState.layoutDirection);
      }
      if (uiState?.activeMode === 'workspace' && uiState.workspaceTabs) {
        const { workspaceId } = uiState.workspaceTabs;
        const ws = useWorkspaceStore.getState().workspaces.find((w) => w.id === workspaceId);
        if (ws) {
          usePaneStore.getState().openWorkspaceTabs(ws.id);
        }
      } else if (uiState?.activeMode === 'collection' && uiState.activeCollection) {
        // Restore the active collection and any collection overview tabs that
        // were open before the reload. Request tabs are not restored (they carry
        // heavy in-flight state); only lightweight collection-type tabs are.
        usePaneStore.getState().switchCollection(uiState.activeCollection);
        for (const saved of uiState.collectionTabs ?? []) {
          const tab: CollectionTab = {
            id: saved.id,
            title: saved.title,
            tabType: 'collection',
            collectionName: saved.collectionName,
            activeSection: (saved.activeSection as CollectionTab['activeSection']) ?? 'overview',
            isDirty: false,
          };
          usePaneStore.getState().openTab(tab);
        }
      } else {
        // No saved state — show workspace overview as the default landing page.
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
              className='sidebar-elevation w-(--sidebar-w) shrink-0 relative z-[5]'
            >
              <ErrorBoundary>
                <CollectionsSidebar />
              </ErrorBoundary>
            </div>
            <div
              role='separator'
              aria-orientation='vertical'
              className='relative h-full w-px shrink-0 cursor-col-resize group z-10 overflow-visible'
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
            >
              <div className='w-px h-full bg-sidebar-border transition-colors group-hover:bg-primary/60' />
              {/* expanded hit area — absolutely positioned, doesn't affect layout */}
              <div className='absolute inset-y-0 -left-[4px] -right-[4px]' />
            </div>
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
