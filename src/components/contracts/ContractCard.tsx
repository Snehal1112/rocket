import { formatDistanceToNow, parseISO } from 'date-fns';
import { AlertTriangle, ArrowRight, Calendar, Clock, Lock, MoreHorizontal } from 'lucide-react';
import { forwardRef } from 'react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import type { Contract } from '@/types/contracts';
import { ContractContextMenu, ContractDropdownMenu } from './ContractContextMenu';
import { ContractStatusChip } from './ContractStatusChip';
import { MetaItem } from './internal/MetaItem';
import { PrimaryAction } from './internal/PrimaryAction';
import { StatusSubline } from './internal/StatusSubline';
import { VersionTag } from './internal/VersionTag';
import { MiniChangelog } from './MiniChangelog';
import { PartyPill } from './PartyPill';
import { ScopeTag } from './ScopeTag';

// All actions a parent can receive from this card.
// 'resign' = re-sign a drifted/breached contract.
export type ContractAction =
  | 'open'
  | 'edit'
  | 'resign'
  | 'publish'
  | 'pause'
  | 'resume'
  | 'renew'
  | 'send_for_review'
  | 'approve'
  | 'reject'
  | 'duplicate'
  | 'export'
  | 'delete'
  | 'view_changelog';

interface ContractCardProps {
  contract: Contract;
  collectionName?: string;
  collectionRoot: string;
  onAction: (action: ContractAction, contractId: string) => void;
  onOpen?: (contractId: string) => void;
  /** Passed by ContractsTab for j/k keyboard navigation highlight */
  focused?: boolean;
  className?: string;
}

// ─── Pure helpers ─────────────────────────────────────────

function formatDate(iso: string): string {
  try {
    return new Date(`${iso}T00:00:00`).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    });
  } catch {
    return iso;
  }
}

function isExpiringSoon(expiresAt: string | null): boolean {
  if (!expiresAt) return false;
  const ms = new Date(`${expiresAt}T00:00:00`).getTime() - Date.now();
  return ms > 0 && ms < 30 * 24 * 60 * 60 * 1000;
}

function lastChangeLabel(updatedAt: string): string {
  try {
    return `Updated ${formatDistanceToNow(parseISO(updatedAt), { addSuffix: true })}`;
  } catch {
    return 'Updated recently';
  }
}

function policyLabel(policy: Contract['policy']): string {
  return (
    ({ strict: 'Strict', lenient: 'Lenient', additive_ok: 'Additive OK' } as const)[
      policy.breakingChangePolicy
    ] ?? policy.breakingChangePolicy
  );
}

// ─── Component ────────────────────────────────────────────

export const ContractCard = forwardRef<HTMLElement, ContractCardProps>(function ContractCard(
  { contract, collectionName, collectionRoot, onAction, onOpen, focused, className },
  ref,
) {
  const statusCount = contract.breachCount > 0 ? contract.breachCount : contract.driftCount;

  function stopPropagation(e: React.MouseEvent) {
    e.stopPropagation();
  }

  return (
    <ContractContextMenu contract={contract} collectionRoot={collectionRoot} onAction={onAction}>
      <article
        ref={ref as React.Ref<HTMLElement>}
        aria-labelledby={`cc-name-${contract.id}`}
        data-status={contract.status}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: card keyboard-navigable via j/k in ContractsTab
        tabIndex={0}
        onClick={() => onOpen?.(contract.id)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') onOpen?.(contract.id);
        }}
        className={cn(
          // Base
          'group relative bg-card border border-border rounded-[var(--radius)]',
          'p-[18px_20px] grid grid-cols-1 md:grid-cols-[1fr_220px] gap-6 mb-[10px]',
          'cursor-pointer transition-[border-color,box-shadow] duration-[120ms]',
          'hover:border-[hsl(var(--border)/1.4)] hover:shadow-[0_1px_3px_rgba(0,0,0,0.06)]',
          'focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50',
          // Status modifiers (spec §7.2)
          contract.status === 'drift' && 'border-l-[3px] border-l-[hsl(var(--warning))] pl-[17px]',
          contract.status === 'breach' &&
            'border-l-[3px] border-l-[hsl(var(--destructive))] pl-[17px] bg-[color-mix(in_oklab,hsl(var(--destructive-soft))_25%,hsl(var(--card)))]',
          contract.status === 'paused' &&
            'bg-[color-mix(in_oklab,hsl(var(--muted))_50%,hsl(var(--card)))]',
          contract.status === 'expired' && 'opacity-75',
          // Keyboard focus highlight from parent
          focused && 'ring-2 ring-ring ring-offset-2 ring-offset-background',
          className,
        )}
      >
        {/* ─── Left column ─────────────────────────────────── */}
        <div className='min-w-0'>
          {/* Title row */}
          <div className='flex items-start justify-between gap-3 mb-2.5'>
            <div className='min-w-0 flex-1'>
              <div className='flex items-center gap-2 text-[15px] font-semibold tracking-tight'>
                <Lock className='w-3.5 h-3.5 text-muted-foreground shrink-0' aria-hidden='true' />
                <span id={`cc-name-${contract.id}`} className='truncate'>
                  {contract.name}
                </span>
                <VersionTag version={contract.version} />
              </div>
              <div className='flex gap-1.5 items-center flex-wrap mt-1 text-xs text-muted-foreground'>
                {collectionName && (
                  <>
                    <span>{collectionName}</span>
                    <span
                      className='w-1 h-1 rounded-full bg-muted-foreground/40'
                      aria-hidden='true'
                    />
                  </>
                )}
                <span>{formatDate(contract.effectiveAt)}</span>
                <StatusSubline contract={contract} />
              </div>
            </div>
            <ContractStatusChip status={contract.status} count={statusCount} />
          </div>

          {/* Parties */}
          <div className='flex items-center gap-2 flex-wrap mb-3'>
            <PartyPill party={contract.provider} partyRole='provider' />
            <ArrowRight className='w-4 h-3.5 text-muted-foreground shrink-0' aria-hidden='true' />
            <PartyPill party={contract.consumers[0]} partyRole='consumer' />
            {contract.consumers.length > 1 && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className='text-xs text-muted-foreground cursor-default select-none'>
                    +{contract.consumers.length - 1} more
                  </span>
                </TooltipTrigger>
                <TooltipContent side='top'>
                  {contract.consumers
                    .slice(1)
                    .map((c) => c.name)
                    .join(', ')}
                </TooltipContent>
              </Tooltip>
            )}
          </div>

          {/* Meta row */}
          <div className='flex gap-5 flex-wrap mb-3'>
            <MetaItem
              icon={<Calendar className='h-3 w-3' />}
              label='Effective'
              value={formatDate(contract.effectiveAt)}
            />
            <MetaItem
              icon={<Clock className='h-3 w-3' />}
              label={contract.expiresAt ? 'Expires' : 'No expiry'}
              value={contract.expiresAt ? formatDate(contract.expiresAt) : '—'}
              warning={isExpiringSoon(contract.expiresAt)}
            />
            {contract.driftCount > 0 && (
              <MetaItem
                icon={<AlertTriangle className='h-3 w-3' />}
                value={`${contract.driftCount} change${contract.driftCount !== 1 ? 's' : ''}`}
                danger={contract.breachCount > 0}
                warning={contract.breachCount === 0}
              />
            )}
          </div>

          {/* Scope + meta tags */}
          <div className='flex gap-1.5 flex-wrap mb-3'>
            <ScopeTag scope={contract.scope} />
            <ScopeTag type='endpoints' count={contract.endpointCount} />
            <ScopeTag type='policy' label={policyLabel(contract.policy)} />
            {contract.policy.uptimeSla !== null && (
              <ScopeTag type='sla' label={String(contract.policy.uptimeSla)} />
            )}
          </div>

          {/* Footer */}
          <div className='mt-1.5 pt-3 border-t border-dashed border-border flex justify-between items-center'>
            <span className='text-[11px] text-muted-foreground'>
              {lastChangeLabel(contract.updatedAt)}
            </span>
            {/* Action buttons — hidden until hover/focus */}
            <div className='flex gap-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity'>
              <PrimaryAction contract={contract} onAction={onAction} />
              <Button
                variant='ghost'
                size='sm'
                className='h-7 text-xs'
                onClick={(e) => {
                  stopPropagation(e);
                  onAction('edit', contract.id);
                }}
                aria-label={`Edit ${contract.name}`}
              >
                Edit
              </Button>
              <ContractDropdownMenu
                contract={contract}
                collectionRoot={collectionRoot}
                onAction={onAction}
              >
                <Button
                  variant='ghost'
                  size='icon'
                  className='h-7 w-7'
                  onClick={stopPropagation}
                  aria-label='More actions'
                  data-more-trigger
                >
                  <MoreHorizontal className='h-3.5 w-3.5' aria-hidden='true' />
                </Button>
              </ContractDropdownMenu>
            </div>
          </div>
        </div>

        {/* ─── Right column — MiniChangelog ────────────────── */}
        <div className='hidden md:block'>
          <MiniChangelog
            entries={contract.changelog}
            status={contract.status}
            onViewAll={() => onAction('view_changelog', contract.id)}
          />
        </div>
      </article>
    </ContractContextMenu>
  );
});
