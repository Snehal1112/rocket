import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchExternalSecretNames } from '@/lib/tauri-api';

// How long scope typing must pause before the secrets are fetched again.
export const SCOPE_DEBOUNCE_MS = 400;

type ListingStatus = 'idle' | 'loading' | 'ready' | 'error';

interface ListingState {
  status: ListingStatus;
  secrets: { id: string; name: string }[];
  error: string | null;
}

const IDLE: ListingState = { status: 'idle', secrets: [], error: null };

interface SettledScope {
  connectionId: string;
  scope: string;
}

// Lists the secrets of a connection and scope, and keeps the list fresh.
export function useSecretListing(args: { connectionId: string; scope: string; enabled: boolean }): {
  status: ListingStatus;
  secrets: { id: string; name: string }[];
  error: string | null;
  refresh: () => void;
} {
  const { connectionId, scope, enabled } = args;
  const [state, setState] = useState<ListingState>(IDLE);
  const [settled, setSettled] = useState<SettledScope>({ connectionId, scope });
  // Bumped whenever the list becomes stale. A fetch that resolves after a bump is dropped.
  const generation = useRef(0);

  // A new connection uses its own scope at once. Only typing in the scope is debounced.
  const settledScope = settled.connectionId === connectionId ? settled.scope : scope;

  useEffect(() => {
    if (settled.connectionId !== connectionId) {
      setSettled({ connectionId, scope });
      return;
    }
    if (settled.scope === scope) return;
    const timer = setTimeout(() => setSettled({ connectionId, scope }), SCOPE_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [connectionId, scope, settled]);

  const load = useCallback((id: string, listScope: string) => {
    const current = ++generation.current;
    setState({ status: 'loading', secrets: [], error: null });
    fetchExternalSecretNames(id, listScope).then(
      (refs) => {
        if (current !== generation.current) return;
        setState({
          status: 'ready',
          secrets: refs.map((ref) => ({ id: ref.secretId, name: ref.name })),
          error: null,
        });
      },
      (e: unknown) => {
        if (current !== generation.current) return;
        setState({ status: 'error', secrets: [], error: String(e) });
      },
    );
  }, []);

  useEffect(() => {
    // An empty settled scope is never fetched, even while the typed scope is still catching up.
    if (!enabled || connectionId === '' || settledScope === '') {
      generation.current += 1;
      setState(IDLE);
      return;
    }
    load(connectionId, settledScope);
    return () => {
      // Drops the response of this run after a dependency change or unmount.
      generation.current += 1;
    };
  }, [enabled, connectionId, settledScope, load]);

  const refresh = useCallback(() => {
    if (!enabled || connectionId === '') return;
    if (scope === settledScope) {
      load(connectionId, scope);
    } else {
      // The effect above fetches once the settled scope catches up.
      setSettled({ connectionId, scope });
    }
  }, [enabled, connectionId, scope, settledScope, load]);

  return { status: state.status, secrets: state.secrets, error: state.error, refresh };
}
