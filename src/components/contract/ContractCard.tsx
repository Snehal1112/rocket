import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { openPath } from '@tauri-apps/plugin-opener';
import { Calendar, FileText, Layers, Paperclip, Pencil, ScrollText, Trash2 } from 'lucide-react';
import { useShallow } from 'zustand/react/shallow';
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
  const changeCount = useContractStore(
    useShallow((s) => s.changelogs[contract.id]?.entries.length ?? 0),
  );
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

  const scopeIcon =
    contract.scope.type === 'collection' ? (
      <Layers className='h-3 w-3 shrink-0' />
    ) : (
      <FileText className='h-3 w-3 shrink-0' />
    );

  function resolveDocPath(relPath: string): string {
    return `${collectionRoot}/${relPath}`;
  }

  function openDocument(docPath: string, title: string) {
    const absPath = resolveDocPath(docPath);
    if (absPath.toLowerCase().endsWith('.pdf')) {
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
        'rounded-lg border bg-card transition-colors group',
        preview ? '' : 'hover:border-primary/40',
      ].join(' ')}
    >
      {/* Header */}
      <div className='flex items-start justify-between gap-3 px-4 pt-3.5 pb-3'>
        <div className='min-w-0 flex-1'>
          <p className='text-sm font-semibold text-foreground truncate leading-snug'>
            {contract.title}
          </p>
          <p className='text-xs text-muted-foreground mt-0.5 truncate'>
            {contract.project}
            {contract.version ? (
              <>
                <span className='mx-1 opacity-40'>·</span>
                <span className='font-mono text-[11px]'>{contract.version}</span>
              </>
            ) : null}
          </p>
        </div>
        <Badge variant={statusVariant} className='shrink-0 text-xs mt-0.5'>
          {statusLabel}
        </Badge>
      </div>

      <Separator />

      {/* Body */}
      <div className='px-4 py-3 space-y-2.5'>
        {/* Parties */}
        <div className='flex items-center gap-1.5 flex-wrap'>
          <span className='inline-flex items-center gap-1.5 bg-secondary rounded-md px-2 py-0.5 text-xs font-medium text-secondary-foreground'>
            <span className='w-1.5 h-1.5 rounded-full bg-violet-500 shrink-0' />
            {contract.provider}
          </span>
          <span className='text-muted-foreground/60 text-xs select-none'>→</span>
          <span className='inline-flex items-center gap-1.5 bg-secondary rounded-md px-2 py-0.5 text-xs font-medium text-secondary-foreground'>
            <span className='w-1.5 h-1.5 rounded-full bg-emerald-500 shrink-0' />
            {contract.consumer}
          </span>
        </div>

        {/* Date range + scope */}
        <div className='flex items-center gap-3 flex-wrap'>
          <span className='inline-flex items-center gap-1.5 text-xs text-muted-foreground'>
            <Calendar className='h-3 w-3 shrink-0' />
            {contract.effectiveDate}
            {contract.expiryDate ? (
              <span className='text-muted-foreground/50'>→ {contract.expiryDate}</span>
            ) : (
              <span className='text-muted-foreground/50'>· no expiry</span>
            )}
          </span>

          <span className='inline-flex items-center gap-1 text-xs text-muted-foreground bg-secondary rounded-md px-1.5 py-0.5'>
            {scopeIcon}
            {scopeLabel}
          </span>
        </div>

        {/* Attached documents */}
        {(contract.documentPaths ?? []).length > 0 && (
          <div className='flex flex-col gap-1'>
            {(contract.documentPaths ?? []).map((p) =>
              preview ? (
                <span
                  key={p}
                  className='flex items-center gap-1.5 text-xs text-muted-foreground min-w-0 max-w-full'
                >
                  <Paperclip className='h-3 w-3 shrink-0' />
                  <span className='truncate min-w-0'>{p.split('/').pop() ?? p}</span>
                </span>
              ) : (
                <button
                  key={p}
                  type='button'
                  className='flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors min-w-0 max-w-full text-left'
                  onClick={() => openDocument(p, contract.title)}
                >
                  <Paperclip className='h-3 w-3 shrink-0' />
                  <span className='truncate min-w-0'>{p.split('/').pop() ?? p}</span>
                </button>
              ),
            )}
          </div>
        )}
      </div>

      {/* Footer */}
      {!preview && (
        <>
          <Separator />
          <div className='flex items-center justify-between px-4 py-2'>
            <span className='inline-flex items-center gap-1.5 text-xs text-muted-foreground'>
              <ScrollText className='h-3 w-3 shrink-0' />
              {changeCount === 0
                ? 'No changes recorded'
                : `${changeCount} change${changeCount === 1 ? '' : 's'} logged`}
            </span>
            <div className='flex items-center gap-0.5'>
              {onViewChangelog && (
                <Button
                  variant='ghost'
                  size='sm'
                  className='h-7 text-xs px-2 text-muted-foreground hover:text-foreground'
                  onClick={onViewChangelog}
                >
                  Changelog
                </Button>
              )}
              {onEdit && (
                <Button
                  variant='ghost'
                  size='sm'
                  className='h-7 w-7 p-0 text-muted-foreground hover:text-foreground'
                  onClick={onEdit}
                >
                  <Pencil className='h-3.5 w-3.5' />
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
