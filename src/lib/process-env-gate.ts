import { toast } from 'sonner';
import { collectionTrustKeys } from '@/lib/queries/collection-trust-queries';
import { getQueryClient } from '@/lib/query-client';
import { getCollectionTrust } from '@/lib/tauri-api';
import { useConsoleStore } from '@/stores/console-store';
import { usePaneStore } from '@/stores/pane-store';

const PLACEHOLDER = '{{process.env.';

/** Marks a request that has no host environment at all, such as a History entry without a collection. */
export const CLOSED_PROCESS_ENV: unique symbol = Symbol('closed-process-env');

/** A collection name, `CLOSED_PROCESS_ENV`, or `undefined` for a scratch request with full access. */
export type ProcessEnvScope = string | typeof CLOSED_PROCESS_ENV | undefined;

/**
 * Warns before a send when the request text uses `{{process.env.NAME}}` and the collection
 * is not allowed host environment access on this computer. The send still goes out with
 * the placeholder unresolved, so this never blocks. A request outside any collection keeps
 * the access and is never warned about. Any failure here is silent.
 *
 * `parts` is the raw request text or the input object that will be sent (URL, headers,
 * params, body, auth fields). Objects are searched through their JSON form.
 */
export async function warnIfProcessEnvWithheld(
  scope: ProcessEnvScope | null,
  parts: unknown[],
  requestName = 'Request',
): Promise<void> {
  if (!scope) return;
  const used = parts.some((part) => {
    if (part == null) return false;
    try {
      const text = typeof part === 'string' ? part : JSON.stringify(part);
      return text.includes(PLACEHOLDER);
    } catch {
      return false;
    }
  });
  if (!used) return;
  if (scope === CLOSED_PROCESS_ENV) {
    const message =
      'Host environment variables are not available for a request reopened from History without a collection, so {{process.env.NAME}} was sent unresolved.';
    useConsoleStore.getState().addScriptEntry({ level: 'warn', message, requestName });
    toast.warning(message);
    return;
  }
  const collection = scope;
  try {
    const trust = await getQueryClient().fetchQuery({
      queryKey: collectionTrustKeys.one(collection),
      queryFn: () => getCollectionTrust(collection),
    });
    if (trust.processEnv.granted) return;
    const message =
      'Host environment variables are not allowed for this collection, so {{process.env.NAME}} was sent unresolved.';
    useConsoleStore.getState().addScriptEntry({ level: 'warn', message, requestName });
    toast.warning(message, {
      action: {
        label: 'Review',
        onClick: () => usePaneStore.getState().openCollectionTab(collection, 'overview'),
      },
    });
  } catch {
    // The trust state is unreadable. The backend already withholds the variables.
  }
}
