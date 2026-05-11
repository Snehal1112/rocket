import { formatDistanceToNow, parseISO } from 'date-fns';
import {
  AlertTriangle,
  Archive,
  ArrowRight,
  Calendar,
  Check,
  Clock,
  GitBranch,
  Infinity as InfinityIcon,
  Lock,
  MessageCircle,
  MoreHorizontal,
  Power,
  TrendingUp,
  User,
  Users,
} from 'lucide-react';
import { forwardRef } from 'react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { track } from '@/lib/telemetry';
import { cn } from '@/lib/utils';
import { useDrawerStore } from '@/stores/contracts/drawerSlice';
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
  | 'review_diff'
  | 'accept_drift'
  | 'open_review'
  | 'remind_reviewers'
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
  | 'archive'
  | 'unarchive';

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

function formatMonthDay(iso: string): string {
  try {
    return new Date(`${iso}T00:00:00`).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return iso;
  }
}

function subtitleDateLabel(contract: Contract): string {
  if (contract.status === 'paused' && contract.pausedAt) {
    return `Paused ${formatDate(contract.pausedAt)}`;
  }
  if (contract.status === 'expired' && contract.expiresAt) {
    return `Expired ${formatDate(contract.expiresAt)}`;
  }
  if (contract.status === 'in_review' && contract.createdAt) {
    try {
      return `Created ${new Date(contract.createdAt).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })}`;
    } catch {
      // fall through
    }
  }
  return `Effective ${formatDate(contract.effectiveAt)}`;
}

/** Bumps the last dot-separated numeric segment: "v2.1" → "v2.2", "1.0.0" → "1.0.1" */
function nextVersion(version: string): string {
  const hasV = version.startsWith('v');
  const v = hasV ? version.slice(1) : version;
  const parts = v.split('.');
  const last = parseInt(parts[parts.length - 1], 10);
  if (!Number.isNaN(last)) {
    parts[parts.length - 1] = String(last + 1);
    return `${hasV ? 'v' : ''}${parts.join('.')}`;
  }
  return `${version}-next`;
}

function driftFooterLabel(contract: Contract): string {
  if (contract.changelog.length === 0) return 'No changes recorded';
  const latest = contract.changelog[0];
  try {
    const timeStr = formatDistanceToNow(parseISO(latest.at), { addSuffix: true }).replace(
      'about ',
      '',
    );
    const byPart = latest.authorName ? ` · by ${latest.authorName}` : '';
    return `Last change ${timeStr}${byPart}`;
  } catch {
    return 'No changes recorded';
  }
}

function reviewFooterLabel(contract: Contract): string {
  try {
    const timeStr = formatDistanceToNow(parseISO(contract.createdAt), { addSuffix: true }).replace(
      'about ',
      '',
    );
    const byPart = contract.createdBy ? ` · by ${contract.createdBy}` : '';
    return `Proposed ${timeStr}${byPart}`;
  } catch {
    return 'Awaiting review';
  }
}

function isExpiringSoon(expiresAt: string | null): boolean {
  if (!expiresAt) return false;
  const ms = new Date(`${expiresAt}T00:00:00`).getTime() - Date.now();
  return ms > 0 && ms < 30 * 24 * 60 * 60 * 1000;
}

function footerLabel(contract: Contract): string {
  if (contract.changelog.length === 0) return 'No changes recorded';
  const latest = contract.changelog[0];
  try {
    const timeStr = formatDistanceToNow(parseISO(latest.at), { addSuffix: false }).replace(
      'about ',
      '',
    );
    const kindLabel =
      latest.kind === 'add' ? 'additive' : latest.kind === 'remove' ? 'removal' : 'modified';
    const breakLabel = latest.isBreaking ? 'breaking' : 'accepted';
    return `Last change ${timeStr} ago · ${kindLabel}, ${breakLabel}`;
  } catch {
    return 'No changes recorded';
  }
}

function policyLabel(policy: Contract['policy']): string {
  const base =
    ({ strict: 'Strict', lenient: 'Standard', additive_ok: 'Additive OK' } as const)[
      policy.breakingChangePolicy
    ] ?? policy.breakingChangePolicy;
  return `${base} · ${policy.noticeDays}-day notice`;
}

// ─── Component ────────────────────────────────────────────

export const ContractCard = forwardRef<HTMLElement, ContractCardProps>(function ContractCard(
  { contract, collectionName, collectionRoot, onAction, onOpen, focused, className },
  ref,
) {
  const openDrawer = useDrawerStore((s) => s.open);
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
          // j/k keyboard navigation highlight — ring-ring colour, no offset gap
          focused && 'ring-2 ring-ring/50',
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
                <span>{subtitleDateLabel(contract)}</span>
                <StatusSubline contract={contract} />
              </div>
            </div>
            <ContractStatusChip status={contract.status} count={statusCount} />
          </div>

          {/* Parties */}
          <div className='flex items-center gap-2 flex-wrap mb-3'>
            <PartyPill party={contract.provider} partyRole='provider' />
            <ArrowRight className='w-4 h-3.5 text-muted-foreground shrink-0' aria-hidden='true' />
            {contract.consumers.length === 1 ? (
              <PartyPill party={contract.consumers[0]} partyRole='consumer' />
            ) : (
              <>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <div className='inline-flex items-center gap-[7px] pl-[4px] pr-[10px] py-1 border border-border rounded-full bg-card text-xs text-foreground cursor-default'>
                      <span className='w-5 h-5 rounded-full bg-primary/20 text-primary text-[10px] font-semibold flex items-center justify-center shrink-0'>
                        +{contract.consumers.length}
                      </span>
                      <span>{contract.consumers.length} consumers</span>
                      <span className='text-[10px] text-muted-foreground font-medium shrink-0'>
                        · Consumer
                      </span>
                    </div>
                  </TooltipTrigger>
                  <TooltipContent side='top'>
                    {contract.consumers.map((c) => c.name).join(', ')}
                  </TooltipContent>
                </Tooltip>
                <span className='text-xs text-muted-foreground truncate max-w-[200px]'>
                  · {contract.consumers.map((c) => c.name).join(', ')}
                </span>
              </>
            )}
          </div>

          {/* Meta row — content varies by status */}
          <div className='flex gap-5 flex-wrap mb-3'>
            {contract.status === 'paused' ? (
              <>
                {contract.pausedBy && (
                  <MetaItem
                    icon={<User className='h-3 w-3' />}
                    label='Paused by'
                    value={contract.pausedBy}
                  />
                )}
                {contract.successorName && (
                  <MetaItem
                    icon={<GitBranch className='h-3 w-3' />}
                    label='Superseded by'
                    value={contract.successorName}
                  />
                )}
                <MetaItem
                  icon={<Power className='h-3 w-3' />}
                  label='Drift detection'
                  value={contract.driftDetectionEnabled === false ? 'off' : 'on'}
                />
              </>
            ) : contract.status === 'expired' ? (
              <>
                {contract.expiresAt && (
                  <MetaItem
                    icon={<Clock className='h-3 w-3' />}
                    label='Expired'
                    value={`${formatDistanceToNow(parseISO(contract.expiresAt))} ago`}
                  />
                )}
                {contract.isArchiveCandidate && (
                  <MetaItem icon={<Archive className='h-3 w-3' />} value='Archive candidate' />
                )}
              </>
            ) : contract.status === 'in_review' ? (
              <>
                <MetaItem
                  icon={<Calendar className='h-3 w-3' />}
                  label='Proposed effective'
                  value={formatMonthDay(contract.effectiveAt)}
                />
                {contract.reviewerCount !== undefined && (
                  <MetaItem
                    icon={<Users className='h-3 w-3' />}
                    value={`${contract.reviewerCount} reviewer${contract.reviewerCount !== 1 ? 's' : ''}`}
                  />
                )}
                {contract.commentCount !== undefined && (
                  <MetaItem
                    icon={<MessageCircle className='h-3 w-3' />}
                    value={`${contract.commentCount} comment${contract.commentCount !== 1 ? 's' : ''}`}
                  />
                )}
              </>
            ) : (
              <>
                <MetaItem
                  icon={<Calendar className='h-3 w-3' />}
                  label='Effective'
                  value={formatDate(contract.effectiveAt)}
                />
                <MetaItem
                  icon={
                    contract.expiresAt ? (
                      <Clock className='h-3 w-3' />
                    ) : (
                      <InfinityIcon className='h-3 w-3' />
                    )
                  }
                  label={contract.expiresAt ? 'Expires' : 'No expiry'}
                  value={contract.expiresAt ? formatDate(contract.expiresAt) : '· evergreen'}
                  warning={isExpiringSoon(contract.expiresAt)}
                />
                {contract.status === 'active' || contract.status === 'expiring_in_30_days' ? (
                  contract.policy.uptimeSla !== null ? (
                    <MetaItem
                      icon={<TrendingUp className='h-3 w-3' />}
                      value={`${contract.policy.uptimeSla}% compliance`}
                      success
                    />
                  ) : (
                    <MetaItem
                      icon={<Check className='h-3 w-3' />}
                      value={`${contract.driftCount} break${contract.driftCount !== 1 ? 's' : ''} in 30d`}
                      success={contract.driftCount === 0}
                    />
                  )
                ) : contract.driftCount > 0 ? (
                  <MetaItem
                    icon={<AlertTriangle className='h-3 w-3' />}
                    value={
                      contract.breachCount > 0
                        ? `${contract.breachCount} breaking change${contract.breachCount !== 1 ? 's' : ''}`
                        : 'Additive drift'
                    }
                    danger={contract.breachCount > 0}
                    warning={contract.breachCount === 0}
                  />
                ) : null}
              </>
            )}
          </div>

          {/* Scope + meta tags — content varies by status */}
          <div className='flex gap-1.5 flex-wrap mb-3'>
            {contract.status === 'expired' ? (
              <>
                <ScopeTag type='endpoints' count={contract.endpointCount} />
                <ScopeTag type='policy' label='Expired — read only' />
              </>
            ) : contract.status === 'paused' ? (
              <>
                <ScopeTag scope={contract.scope} />
                <ScopeTag type='endpoints' count={contract.endpointCount} />
              </>
            ) : contract.status === 'in_review' ? (
              <>
                <ScopeTag type='endpoints' count={contract.endpointCount} />
                <ScopeTag
                  type='policy'
                  label={
                    contract.policy.breakingChangePolicy === 'strict'
                      ? 'Strict · Both parties sign'
                      : policyLabel(contract.policy)
                  }
                />
              </>
            ) : (
              <>
                <ScopeTag scope={contract.scope} />
                <ScopeTag type='endpoints' count={contract.endpointCount} />
                <ScopeTag type='policy' label={policyLabel(contract.policy)} />
                {contract.policy.uptimeSla !== null && (
                  <ScopeTag type='sla' label={String(contract.policy.uptimeSla)} />
                )}
              </>
            )}
          </div>

          {/* Footer — content and actions vary by status */}
          <div className='mt-1.5 pt-3 border-t border-dashed border-border flex justify-between items-center gap-3'>
            {contract.status === 'paused' ? (
              <>
                <span className='text-[11px] text-muted-foreground italic truncate'>
                  {contract.pauseReason ? (
                    <>
                      Reason: {contract.pauseReason}
                      {contract.successorName && (
                        <>
                          {' '}
                          <strong className='font-semibold not-italic text-foreground/80'>
                            {contract.successorName}
                          </strong>
                        </>
                      )}
                    </>
                  ) : (
                    'Contract is paused'
                  )}
                </span>
                <div className='flex gap-1 shrink-0'>
                  <PrimaryAction contract={contract} onAction={onAction} />
                  <Button
                    variant='ghost'
                    size='sm'
                    className='h-7 text-xs'
                    onClick={(e) => {
                      stopPropagation(e);
                      onAction('archive', contract.id);
                    }}
                  >
                    Archive
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
              </>
            ) : contract.status === 'expired' ? (
              <>
                <span className='text-[11px] text-muted-foreground italic truncate'>
                  {contract.lastRenewalAttemptAt && contract.lastRenewalDeclined
                    ? `Last renewal attempt declined · ${formatMonthDay(contract.lastRenewalAttemptAt)}`
                    : contract.expiresAt
                      ? `Expired ${formatDate(contract.expiresAt)}`
                      : 'Contract expired'}
                </span>
                <div className='flex gap-1 shrink-0'>
                  <PrimaryAction contract={contract} onAction={onAction} />
                  <Button
                    variant='ghost'
                    size='sm'
                    className='h-7 text-xs'
                    onClick={(e) => {
                      stopPropagation(e);
                      onAction('duplicate', contract.id);
                    }}
                  >
                    Clone as new
                  </Button>
                  <Button
                    variant='ghost'
                    size='sm'
                    className='h-7 text-xs text-[hsl(var(--warning))] hover:text-[hsl(var(--warning))] hover:bg-[hsl(var(--warning)/0.08)]'
                    onClick={(e) => {
                      stopPropagation(e);
                      onAction('archive', contract.id);
                    }}
                  >
                    Archive
                  </Button>
                </div>
              </>
            ) : contract.status === 'drift' || contract.status === 'breach' ? (
              <>
                <span className='flex items-center gap-1 text-[11px] text-muted-foreground truncate'>
                  <Clock className='h-3 w-3 shrink-0' aria-hidden='true' />
                  {driftFooterLabel(contract)}
                </span>
                <div className='flex gap-1 shrink-0'>
                  <PrimaryAction contract={contract} onAction={onAction} />
                  <Button
                    variant='ghost'
                    size='sm'
                    className='h-7 text-xs'
                    onClick={(e) => {
                      stopPropagation(e);
                      onAction('accept_drift', contract.id);
                    }}
                  >
                    {contract.status === 'breach'
                      ? `Propose ${nextVersion(contract.version)}`
                      : `Accept as ${nextVersion(contract.version)}`}
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
              </>
            ) : contract.status === 'in_review' ? (
              <>
                <span className='flex items-center gap-1 text-[11px] text-muted-foreground truncate'>
                  <Clock className='h-3 w-3 shrink-0' aria-hidden='true' />
                  {reviewFooterLabel(contract)}
                </span>
                <div className='flex gap-1 shrink-0'>
                  <PrimaryAction contract={contract} onAction={onAction} />
                  <Button
                    variant='ghost'
                    size='sm'
                    className='h-7 text-xs'
                    onClick={(e) => {
                      stopPropagation(e);
                      onAction('remind_reviewers', contract.id);
                    }}
                  >
                    Remind reviewers
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
              </>
            ) : (
              <>
                <span className='flex items-center gap-1 text-[11px] text-muted-foreground'>
                  <Check
                    className={cn(
                      'h-3 w-3 shrink-0',
                      contract.changelog.length === 0 && 'text-[hsl(var(--success))]',
                    )}
                    aria-hidden='true'
                  />
                  {footerLabel(contract)}
                </span>
                <div className='flex gap-1 shrink-0'>
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
              </>
            )}
          </div>
        </div>

        {/* ─── Right column — MiniChangelog ────────────────── */}
        <div className='hidden md:block'>
          <MiniChangelog
            entries={contract.changelog}
            status={contract.status}
            effectiveAt={contract.effectiveAt}
            endpointCount={contract.endpointCount}
            consumersCount={contract.consumers.length}
            successorName={contract.successorName}
            onViewAll={() => {
              try {
                track('contracts.changelog_drawer_opened', {
                  contractId: contract.id,
                  source: 'card_link',
                });
              } catch {
                /* noop */
              }
              openDrawer(contract.id);
            }}
          />
        </div>
      </article>
    </ContractContextMenu>
  );
});
