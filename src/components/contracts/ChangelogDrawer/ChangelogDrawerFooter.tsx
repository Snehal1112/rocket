import { ExternalLink } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import type { DayGroup } from '@/hooks/useChangelogEntries';
import { track } from '@/lib/telemetry';
import type { Contract } from '@/types/contracts';

interface ChangelogDrawerFooterProps {
  contract: Contract;
  shownCount: number;
  groups: DayGroup[];
}

function csvCell(v: string): string {
  return `"${v.replace(/"/g, '""')}"`;
}

function exportCsv(contract: Contract, groups: DayGroup[]) {
  const allEntries = groups.flatMap((g) => g.items);
  const rows = [
    ['at', 'kind', 'isBreaking', 'requestMethod', 'requestPath', 'summary', 'author'],
    ...allEntries.map((e) => [
      csvCell(e.at),
      csvCell(e.kind),
      csvCell(String(e.isBreaking)),
      csvCell(e.requestMethod ?? ''),
      csvCell(e.requestPath ?? ''),
      csvCell(e.summary),
      csvCell(e.authorName ?? ''),
    ]),
  ];
  const csv = rows.map((r) => r.join(',')).join('\n');
  const blob = new Blob([csv], { type: 'text/csv' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `changelog-${contract.name.replace(/\s+/g, '-')}-${contract.version}.csv`;
  a.style.display = 'none';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
  try {
    track('contracts.changelog_exported', {
      contractId: contract.id,
      entryCount: allEntries.length,
    });
  } catch {
    /* noop */
  }
}

function formatSince(iso: string): string {
  try {
    return new Date(`${iso}T00:00:00`).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return iso;
  }
}

export function ChangelogDrawerFooter({
  contract,
  shownCount,
  groups,
}: ChangelogDrawerFooterProps) {
  const totalCount = contract.changelog.length;

  return (
    <div className='px-5 py-2.5 border-t border-border flex justify-between items-center text-[11px] text-muted-foreground'>
      <span>
        Showing {shownCount} of {totalCount} · since {formatSince(contract.effectiveAt)}
      </span>
      <div className='flex items-center gap-1'>
        <Button
          variant='ghost'
          size='sm'
          onClick={() => {
            try {
              track('contracts.changelog_open_as_tab', { contractId: contract.id });
            } catch {
              /* noop */
            }
            toast('Coming soon');
          }}
        >
          Open as tab <ExternalLink className='w-3 h-3 ml-1' aria-hidden='true' />
        </Button>
        <Button variant='ghost' size='sm' onClick={() => exportCsv(contract, groups)}>
          Export CSV
        </Button>
      </div>
    </div>
  );
}
