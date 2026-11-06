import { useEffect } from 'react';
import { toast } from 'sonner';
import { UPDATE_TOAST_ID, UpdateToast } from '@/components/updates/UpdateToast';
import { createListenerScope } from '@/lib/listen-scope';
import { getInstallKind, onUpdateAvailable, onUpdateProgress } from '@/lib/tauri-api';
import { useUpdateStore } from '@/stores/update-store';

// Versions that already got a toast, so a repeated event stays quiet.
const announced = new Set<string>();

export function resetAnnouncedUpdates(): void {
  announced.clear();
}

// Shows or refreshes the one update toast. The fixed id makes it update in place.
function showToast(): void {
  const { available } = useUpdateStore.getState();
  if (!available) return;
  toast.custom(() => <UpdateToast info={available} />, {
    id: UPDATE_TOAST_ID,
    duration: Number.POSITIVE_INFINITY,
  });
}

// Listens for update events and shows the update toast. Mount once.
export function useUpdateNotice(): void {
  useEffect(() => {
    const scope = createListenerScope();

    getInstallKind()
      .then((kind) => {
        if (!scope.isDisposed()) useUpdateStore.getState().setInstallKind(kind);
      })
      .catch(() => undefined);

    scope.keep(
      onUpdateAvailable((info) => {
        const { status } = useUpdateStore.getState();
        // A running or finished download keeps its update. The next check announces again.
        if (status === 'downloading' || status === 'ready') return;
        if (announced.has(info.version)) return;
        // A new version starts over, so old progress or errors do not leak into its toast.
        useUpdateStore.setState({ available: info, status: 'idle', progress: null, error: null });
        announced.add(info.version);
        showToast();
      }),
    );
    scope.keep(onUpdateProgress((p) => useUpdateStore.getState().setProgress(p)));

    // Progress, status and error changes refresh the toast that is already shown.
    const unsubscribe = useUpdateStore.subscribe((s, prev) => {
      if (!s.available || !announced.has(s.available.version)) return;
      if (s.available !== prev.available) return;
      if (s.status === prev.status && s.progress === prev.progress && s.error === prev.error)
        return;
      showToast();
    });

    return () => {
      scope.dispose();
      unsubscribe();
    };
  }, []);
}
