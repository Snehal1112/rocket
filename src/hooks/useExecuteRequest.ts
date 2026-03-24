import { useState, useCallback } from 'react';
import { sendRequest } from '@/lib/execute-request';
import type { RequestState } from '@/types/pane-types';

// Thin React wrapper around sendRequest that tracks the in-flight state.
export function useExecuteRequest(tabId: string) {
  const [sending, setSending] = useState(false);

  const send = useCallback(async (request: RequestState) => {
    setSending(true);
    try {
      await sendRequest(tabId, request);
    } finally {
      setSending(false);
    }
  }, [tabId]);

  return { send, sending };
}
