import { CircleSlash, Clock, ShieldAlert, ShieldCheck, UserRoundX } from 'lucide-react';
import type { ReactNode } from 'react';
import { DiffViewer } from '@/components/git/DiffViewer';
import { Badge } from '@/components/ui/badge';
import { NO_ANSWER_TEXT, verdictLabel } from '@/lib/network-contract-check-utils';
import type { ContractChange, ContractVerdict } from '@/lib/tauri-api';
import { contractCheckRunKey, useContractCheckRunsStore } from '@/stores/contract-check-runs-store';
import { type ContractCheckResult, useNetworkStore } from '@/stores/network-store';

export const EXPIRED_TEXT = 'No answer, the check expired';

/** One diff of the whole change list. Values are the redacted ones that went out. */
export function previewContents(changes: ContractChange[]): { before: string; after: string } {
  const line = (c: ContractChange, v: string) => `${c.method} ${c.pathTemplate} ${c.field}: ${v}`;
  const before = changes.filter((c) => c.oldValue !== null).map((c) => line(c, c.oldValue ?? ''));
  const after = changes.filter((c) => c.newValue !== null).map((c) => line(c, c.newValue ?? ''));
  return { before: before.join('\n'), after: after.join('\n') };
}

interface Summary {
  icon: ReactNode;
  label: string;
  variant: 'success' | 'destructive' | 'outline' | 'secondary';
  /** The fields that break, for an answer with a break. */
  fields: string[];
}

const ICON = 'h-3 w-3';

function overallVerdict(result: ContractCheckResult): ContractVerdict {
  if (result.rows.some((r) => r.verdict === 'break')) return 'break';
  if (result.rows.some((r) => r.verdict === 'pass')) return 'pass';
  return 'not_used';
}

export function summarize(
  result: ContractCheckResult | undefined,
  changes: ContractChange[],
): Summary {
  const plain = (icon: ReactNode, label: string, variant: Summary['variant']): Summary => ({
    icon,
    label,
    variant,
    fields: [],
  });
  if (!result)
    return plain(<Clock className={ICON} aria-hidden='true' />, NO_ANSWER_TEXT, 'outline');
  if (result.outcome === 'expired') {
    return plain(<Clock className={ICON} aria-hidden='true' />, EXPIRED_TEXT, 'outline');
  }
  if (result.outcome === 'declined') {
    return plain(<UserRoundX className={ICON} aria-hidden='true' />, 'Declined', 'secondary');
  }
  if (result.outcome === 'busy') {
    return plain(<Clock className={ICON} aria-hidden='true' />, 'Busy', 'secondary');
  }
  const verdict = overallVerdict(result);
  if (verdict === 'break') {
    // The reply carries only the index, so the field comes from our own change list.
    const fields = result.rows
      .filter((r) => r.verdict === 'break')
      .map((r) => changes[r.index]?.field)
      .filter((f): f is string => typeof f === 'string');
    return {
      icon: <ShieldAlert className={ICON} aria-hidden='true' />,
      label: verdictLabel('break'),
      variant: 'destructive',
      fields,
    };
  }
  if (verdict === 'pass') {
    return plain(
      <ShieldCheck className={ICON} aria-hidden='true' />,
      verdictLabel('pass'),
      'success',
    );
  }
  return plain(
    <CircleSlash className={ICON} aria-hidden='true' />,
    verdictLabel('not_used'),
    'outline',
  );
}

/**
 * The preview of the offered changes with one row per asked teammate beside it. A row reads
 * "No answer yet" until the teammate replies. Everything comes from the stores, so the rows
 * survive closing and reopening the view.
 */
export function ContractCheckResults({
  collectionId,
  contractId,
}: {
  collectionId: string;
  contractId: string;
}) {
  const run = useContractCheckRunsStore(
    (s) => s.runs[contractCheckRunKey(collectionId, contractId)],
  );
  const results = useNetworkStore((s) => s.contractCheckResults);
  if (!run) return null;
  const { before, after } = previewContents(run.changes);
  return (
    <section
      aria-label='Contract check results'
      className='grid gap-3 border-b px-6 py-4 md:grid-cols-[minmax(0,1fr)_minmax(0,18rem)]'
    >
      <div className='h-40 overflow-hidden rounded-md border'>
        {/* The key is needed because DiffViewer copies its prop into state once. */}
        <DiffViewer
          key={run.targets.map((t) => t.checkId).join(',')}
          hideStageToggle
          diffState={{
            filePath: 'contract-check.txt',
            repositoryId: '',
            repositoryLabel: 'Contract check',
            oldContent: before,
            newContent: after,
            status: 'modified',
            isStaged: false,
          }}
        />
      </div>
      <ul className='space-y-2' aria-label='Teammates'>
        {run.targets.map((t) => {
          const s = summarize(results[t.checkId]?.[t.deviceId], run.changes);
          return (
            <li key={t.checkId} className='rounded-md border p-2'>
              <div className='flex items-center justify-between gap-2'>
                <span className='min-w-0 break-words text-sm font-medium'>{t.name}</span>
                <Badge variant={s.variant} className='shrink-0 gap-1'>
                  {s.icon}
                  {s.label}
                </Badge>
              </div>
              {s.fields.length > 0 && (
                <p className='mt-1 break-all font-mono text-2xs text-muted-foreground'>
                  {s.fields.join(', ')}
                </p>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
