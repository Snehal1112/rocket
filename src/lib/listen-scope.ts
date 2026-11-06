export interface ListenerScope {
  /** Keeps the unlisten function of a Tauri `listen` call that is still registering. */
  keep: (registration: Promise<() => void>) => void;
  isDisposed: () => boolean;
  /** Unlistens everything kept so far. A listener that registers later is unlistened at once. */
  dispose: () => void;
}

// One scope per effect run. Call `dispose` in the effect cleanup.
export function createListenerScope(): ListenerScope {
  let disposed = false;
  const unlisteners: (() => void)[] = [];

  return {
    keep(registration) {
      registration
        .then((unlisten) => {
          // The component may unmount before the promise resolves.
          if (disposed) unlisten();
          else unlisteners.push(unlisten);
        })
        .catch(() => undefined);
    },
    isDisposed: () => disposed,
    dispose() {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
      unlisteners.length = 0;
    },
  };
}
