import { toast } from 'sonner';
import { collectionTrustKeys } from '@/lib/queries/collection-trust-queries';
import { getQueryClient } from '@/lib/query-client';
import { getCollectionTrust } from '@/lib/tauri-api';
import { useConsoleStore } from '@/stores/console-store';
import { usePaneStore } from '@/stores/pane-store';

const PLACEHOLDER = '{{process.env.';

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
  collection: string | null | undefined,
  parts: unknown[],
  requestName = 'Request',
): Promise<void> {
  if (!collection) return;
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
