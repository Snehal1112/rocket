import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { openPath } from '@tauri-apps/plugin-opener';
import { Paperclip, Trash2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import type { Contract } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';

interface ContractCardProps {
  contract: Contract;
  collectionRoot: string;
  preview?: boolean;
  onViewChangelog?: () => void;
  onEdit?: () => void;
  onDelete?: () => void;
}

export function ContractCard({
  contract,
  collectionRoot,
  preview = false,
  onViewChangelog,
  onEdit,
  onDelete,
}: ContractCardProps) {
  const contractStatus = useContractStore((s) => s.contractStatus);
  const changelogs = useContractStore((s) => s.changelogs);
  const status = contractStatus(contract);

  const statusVariant =
    status === 'expired' ? 'destructive' : status === 'expiring' ? 'warning' : 'default';

  const statusLabel =
    status === 'expired' ? 'Expired' : status === 'expiring' ? 'Expiring soon' : 'Active';

  const scopeLabel =
    contract.scope.type === 'collection'
      ? 'Entire collection'
      : contract.scope.type === 'folder'
        ? `Folder: ${contract.scope.rel_path}`
        : `Request: ${contract.scope.rel_path}`;

  const changeCount = changelogs[contract.id]?.entries.length ?? 0;

  // Resolve a stored relative path to an absolute path under the collection root.
  function resolveDocPath(relPath: string): string {
    return `${collectionRoot}/${relPath}`;
  }

  // Opens a PDF in a native webview window; all other types go to the OS default app.
  function openDocument(docPath: string, title: string) {
    const absPath = resolveDocPath(docPath);
    if (absPath.toLowerCase().endsWith('.pdf')) {
      // Use a stable label derived from the contract id so re-clicking focuses the same window.
      const label = `pdf-${contract.id}`;
      const existing = WebviewWindow.getByLabel(label);
      existing.then((win) => {
        if (win) {
          win.setFocus();
        } else {
          new WebviewWindow(label, {
            url: `file://${absPath}`,
            title,
            width: 900,
            height: 1100,
            resizable: true,
          });
        }
      });
    } else {
      openPath(absPath);
    }
  }

  return (
    <div
      className={[
        'rounded-lg border bg-card p-4 space-y-3 transition-colors',
        preview ? '' : 'hover:border-primary/40',
      ].join(' ')}
    >
      {/* Header row: title + status chip */}
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0'>
          <p className='text-sm font-medium text-foreground truncate'>{contract.title}</p>
          <p className='text-xs text-muted-foreground mt-0.5'>
            {contract.project}
            {contract.version ? ` · ${contract.version}` : ''}
          </p>
        </div>
        <Badge variant={statusVariant} className='shrink-0 text-xs'>
          {statusLabel}
        </Badge>
      </div>

      {/* Parties: pill badges with coloured dots */}
      <div className='flex items-center gap-2 flex-wrap'>
        <span className='inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs'>
          <span className='w-2 h-2 rounded-full bg-violet-500 shrink-0' />
          {contract.provider}
        </span>
        <span className='text-muted-foreground text-xs'>→</span>
        <span className='inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs'>
          <span className='w-2 h-2 rounded-full bg-emerald-500 shrink-0' />
          {contract.consumer}
        </span>
      </div>

      {/* Date range */}
      <div className='flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground'>
        <span>Effective {contract.effectiveDate}</span>
        {contract.expiryDate && <span>Expires {contract.expiryDate}</span>}
        {!contract.expiryDate && <span>No expiry</span>}
      </div>

      {/* Scope badge */}
      <div>
        <span className='inline-block text-xs bg-secondary text-muted-foreground rounded-full px-2.5 py-0.5'>
          {scopeLabel}
        </span>
      </div>

      {/* Attached documents — guard against undefined for pre-migration contracts */}
      {(contract.documentPaths ?? []).length > 0 && (
        <div className='flex flex-col gap-1'>
          {(contract.documentPaths ?? []).map((p) =>
            preview ? (
              <span
                key={p}
                className='inline-flex items-center gap-1.5 text-xs text-muted-foreground'
              >
                <Paperclip className='h-3 w-3 shrink-0' />
                <span className='truncate'>{p.split('/').pop() ?? p}</span>
              </span>
            ) : (
              <button
                key={p}
                type='button'
                className='inline-flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors'
                onClick={() => openDocument(p, contract.title)}
              >
                <Paperclip className='h-3 w-3 shrink-0' />
                <span className='truncate'>{p.split('/').pop() ?? p}</span>
              </button>
            ),
          )}
        </div>
      )}

      {/* Footer action row — hidden in preview mode */}
      {!preview && (
        <>
          <Separator />
          <div className='flex items-center justify-between'>
            <span className='text-xs text-muted-foreground'>
              {changeCount === 0
                ? 'No changes recorded'
                : `${changeCount} change${changeCount === 1 ? '' : 's'} logged`}
            </span>
            <div className='flex items-center gap-1'>
              {onViewChangelog && (
                <Button variant='ghost' size='sm' className='h-7 text-xs' onClick={onViewChangelog}>
                  View changelog
                </Button>
              )}
              {onEdit && (
                <Button variant='ghost' size='sm' className='h-7 text-xs' onClick={onEdit}>
                  Edit
                </Button>
              )}
              {onDelete && (
                <Button
                  variant='ghost'
                  size='sm'
                  className='h-7 w-7 p-0 text-muted-foreground hover:text-destructive'
                  onClick={onDelete}
                >
                  <Trash2 className='h-3.5 w-3.5' />
                </Button>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
