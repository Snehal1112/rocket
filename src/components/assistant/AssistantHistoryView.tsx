import { Trash2 } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import {
  type ConversationRecord,
  type ConversationSummary,
  clearConversations,
  deleteConversation,
  getConversation,
  listConversations,
  onAssistantConversationDeleted,
  onAssistantConversationSaved,
  onAssistantConversationsCleared,
} from '@/lib/tauri-api';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { AssistantConversationView } from './AssistantConversationView';

export const HISTORY_PRIVACY_NOTICE =
  'Conversations are stored on this computer, inside this workspace, and are never committed to git. Secret variable values are masked. A secret you paste into a prompt is stored as typed. Vault values and collections linked from outside the workspace are not scrubbed. Delete and Clear all remove the stored conversations.';

export const HISTORY_EMPTY_TEXT = 'No saved conversations in this workspace yet.';

const UNTITLED = 'Untitled conversation';

/** The stored conversations of the active workspace. */
export function AssistantHistoryView({ onContinued }: { onContinued?: () => void }) {
  const workspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const { data: agentConfigs = [] } = useAgentConfigs();
  // The list is tagged with its workspace, so another workspace's list is never shown.
  const [list, setList] = useState<{
    workspaceId: string | undefined;
    items: ConversationSummary[];
    loaded: boolean;
    failed: boolean;
  }>({ workspaceId, items: [], loaded: false, failed: false });
  const [open, setOpen] = useState<ConversationRecord>();
  const [confirmClear, setConfirmClear] = useState(false);
  const [busyIds, setBusyIds] = useState<ReadonlySet<string>>(new Set());
  const [clearing, setClearing] = useState(false);
  const workspaceRef = useRef(workspaceId);
  workspaceRef.current = workspaceId;
  // Counters let a late response see that a newer request or a switch happened.
  const listRequest = useRef(0);
  const openRequest = useRef(0);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const shown = list.workspaceId === workspaceId ? list : undefined;
  const items = shown?.items ?? [];
  const loaded = shown?.loaded ?? false;
  const failed = shown?.failed ?? false;

  const reload = useCallback(async () => {
    const request = ++listRequest.current;
    const forWorkspace = workspaceRef.current;
    let next: ConversationSummary[] = [];
    let didFail = false;
    try {
      next = await listConversations();
    } catch {
      didFail = true;
    }
    if (
      !mounted.current ||
      request !== listRequest.current ||
      forWorkspace !== workspaceRef.current
    ) {
      return;
    }
    setList({ workspaceId: forWorkspace, items: next, loaded: true, failed: didFail });
  }, []);

  // A workspace switch closes the open conversation and loads the other list.
  // biome-ignore lint/correctness/useExhaustiveDependencies: workspaceId is a change trigger only.
  useEffect(() => {
    openRequest.current += 1;
    setOpen(undefined);
    setBusyIds(new Set());
    setConfirmClear(false);
    void reload();
  }, [workspaceId, reload]);

  // A finished save or another window changes the list without a reopen.
  useEffect(() => {
    let disposed = false;
    const guard =
      <T,>(handler: (event: T) => void) =>
      (event: T) => {
        if (!disposed) handler(event);
      };
    const registrations = [
      onAssistantConversationSaved(guard(() => void reload())),
      onAssistantConversationDeleted(
        guard((e) => {
          setOpen((current) => (current?.id === e.conversation_id ? undefined : current));
          void reload();
        }),
      ),
      onAssistantConversationsCleared(
        guard(() => {
          openRequest.current += 1;
          setOpen(undefined);
          void reload();
        }),
      ),
    ];
    const settled = Promise.allSettled(registrations).then((results) =>
      results.flatMap((result) => (result.status === 'fulfilled' ? [result.value] : [])),
    );
    return () => {
      disposed = true;
      void settled.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, [reload]);

  const agentLabel = (id: string) => agentConfigs.find((c) => c.id === id)?.label ?? id;
  const continuedFromLabel = (id: string) => {
    const from = items.find((i) => i.id === id);
    return from
      ? `Continued from "${from.title || UNTITLED}"`
      : 'Continued from an earlier conversation';
  };

  const openConversation = async (id: string) => {
    const request = ++openRequest.current;
    try {
      const record = await getConversation(id);
      if (mounted.current && request === openRequest.current) setOpen(record);
    } catch {
      if (mounted.current && request === openRequest.current) {
        toast.error('Could not open the conversation.');
      }
    }
  };

  const remove = async (id: string) => {
    if (busyIds.has(id)) return;
    setBusyIds((current) => new Set(current).add(id));
    try {
      await deleteConversation(id);
    } catch {
      toast.error('Could not delete the conversation.');
    } finally {
      if (mounted.current) {
        setBusyIds((current) => {
          const next = new Set(current);
          next.delete(id);
          return next;
        });
      }
      void reload();
    }
  };

  const clearAll = async () => {
    setConfirmClear(false);
    setClearing(true);
    try {
      await clearConversations();
      openRequest.current += 1;
      setOpen(undefined);
    } catch {
      toast.error('Could not clear the history.');
    } finally {
      if (mounted.current) setClearing(false);
      void reload();
    }
  };

  if (open) {
    return (
      <AssistantConversationView
        record={open}
        agentLabel={agentLabel(open.agentConfigId)}
        onBack={() => setOpen(undefined)}
        onContinued={onContinued}
      />
    );
  }

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='flex items-center gap-2 border-b px-3 py-2'>
        <span className='flex-1 text-sm font-medium'>History</span>
        <Button
          variant='ghost'
          size='sm'
          className='h-7 text-xs'
          disabled={items.length === 0 || clearing}
          onClick={() => setConfirmClear(true)}
        >
          Clear all
        </Button>
      </div>
      <p className='border-b px-3 py-2 text-xs text-muted-foreground'>{HISTORY_PRIVACY_NOTICE}</p>
      <ScrollArea className='min-h-0 flex-1'>
        <ul className='flex flex-col gap-1 p-2'>
          {!loaded && <li className='px-1 text-xs text-muted-foreground'>Loading…</li>}
          {loaded && failed && (
            <li role='alert' className='flex items-center gap-2 px-1 text-xs text-destructive'>
              <span>Could not load the conversations.</span>
              <Button
                variant='outline'
                size='sm'
                className='h-6 text-xs'
                onClick={() => void reload()}
              >
                Retry
              </Button>
            </li>
          )}
          {loaded && !failed && items.length === 0 && (
            <li className='px-1 text-xs text-muted-foreground'>{HISTORY_EMPTY_TEXT}</li>
          )}
          {items.map((item) => (
            <li key={item.id} className='flex items-start gap-1'>
              <Button
                variant='ghost'
                className='h-auto min-w-0 flex-1 flex-col items-start gap-0.5 px-2 py-1.5 text-left'
                onClick={() => void openConversation(item.id)}
              >
                <span className='w-full truncate text-sm'>{item.title || UNTITLED}</span>
                <span className='text-xs text-muted-foreground'>
                  {`${agentLabel(item.agentConfigId)} · ${item.messageCount} ${item.messageCount === 1 ? 'item' : 'items'} · ${formatRelativeTime(item.updatedAt)}`}
                </span>
                {item.continuedFrom && (
                  <span className='text-xs text-muted-foreground'>
                    {continuedFromLabel(item.continuedFrom)}
                  </span>
                )}
              </Button>
              <Button
                variant='ghost'
                size='icon'
                className='h-7 w-7 shrink-0'
                aria-label={`Delete "${item.title || UNTITLED}"`}
                disabled={busyIds.has(item.id)}
                onClick={() => void remove(item.id)}
              >
                <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
              </Button>
            </li>
          ))}
        </ul>
      </ScrollArea>
      <AlertDialog open={confirmClear} onOpenChange={setConfirmClear}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete all conversations?</AlertDialogTitle>
            <AlertDialogDescription>
              Every stored conversation of this workspace is deleted. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void clearAll()}>Delete all</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
