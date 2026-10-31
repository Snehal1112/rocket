import type { ReactNode } from 'react';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { SettingsPage } from '@/components/settings/SettingsPage';
import { useSettingsStore } from '@/stores/settings-store';

// The editor area. Settings opens on top of it, so open tabs stay mounted and come back unchanged.
export function MainArea({ children }: { children: ReactNode }) {
  const settingsOpen = useSettingsStore((s) => s.open);
  return (
    <main className='relative flex-1 flex flex-col min-w-0 overflow-hidden'>
      {/* Inert while covered, so Tab cannot move focus into the hidden tabs. */}
      {/* Isolate keeps z-indexed editor widgets from painting above Settings. */}
      <div inert={settingsOpen} className='isolate flex min-h-0 flex-1 flex-col'>
        {children}
      </div>
      {settingsOpen && (
        // Below the sidebar (z-5) so the sidebar shadow shows over Settings.
        <div className='absolute inset-0 z-[1]'>
          <ErrorBoundary>
            <SettingsPage />
          </ErrorBoundary>
        </div>
      )}
    </main>
  );
}
