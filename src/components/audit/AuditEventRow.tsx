import { Badge } from '@/components/ui/badge';
import type { SecurityAuditEvent } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

interface AuditEventRowProps {
  event: SecurityAuditEvent;
  className?: string;
}

// Format a timestamp as a short relative-time string (e.g. "5m ago").
function formatRelative(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return iso;
  const diffSec = Math.round((Date.now() - then) / 1000);
  const abs = Math.abs(diffSec);
  const suffix = diffSec >= 0 ? 'ago' : 'from now';
  if (abs < 60) return `${abs}s ${suffix}`;
  if (abs < 3600) return `${Math.round(abs / 60)}m ${suffix}`;
  if (abs < 86400) return `${Math.round(abs / 3600)}h ${suffix}`;
  if (abs < 2592000) return `${Math.round(abs / 86400)}d ${suffix}`;
  if (abs < 31536000) return `${Math.round(abs / 2592000)}mo ${suffix}`;
  return `${Math.round(abs / 31536000)}y ${suffix}`;
}

function summarize(event: SecurityAuditEvent): string {
  const k = event.event;
  switch (k.kind) {
    case 'contract_attached':
      return `Attached contract ${k.contractId} to ${k.collection} (${k.scope})`;
    case 'contract_deleted':
      return `Deleted contract ${k.contractId} from ${k.collection}`;
    case 'contract_violation':
      return `Contract ${k.contractId} violation: ${k.field} in ${k.requestPath}`;
    case 'collection_deleted':
      return `Deleted collection ${k.collection}`;
    case 'collection_exported':
      return `Exported collection ${k.collection} to ${k.destination}`;
    case 'secret_variable_written':
      return `Secret variable "${k.variableKey}" written in ${k.environment}`;
    case 'sensitive_auth_used':
      return `${k.authType} auth used on ${k.collection}/${k.requestPath}`;
    case 'audit_evidence_exported':
      return `Exported ${k.count} audit events`;
    case 'audit_chain_broken':
      return `Audit chain broken at event ${k.atEventId}`;
  }
}

export function AuditEventRow({ event, className }: AuditEventRowProps) {
  const relative = formatRelative(event.occurredAt);
  return (
    <tr className={cn('border-b border-border/60 last:border-0', className)}>
      <td className='py-2 px-3 text-xs text-muted-foreground whitespace-nowrap'>
        <time dateTime={event.occurredAt} title={event.occurredAt}>
          {relative}
        </time>
      </td>
      <td className='py-2 px-3 text-xs text-foreground'>{summarize(event)}</td>
      <td className='py-2 px-3'>
        <div className='flex flex-wrap gap-1'>
          {event.controls.map((c) => (
            <Badge
              key={`${c.framework}-${c.code}`}
              variant='secondary'
              className='text-[10px] font-mono'
              title={c.title}
            >
              {c.framework.toUpperCase()} {c.code}
            </Badge>
          ))}
        </div>
      </td>
      <td className='py-2 px-3 text-xs text-muted-foreground font-mono truncate max-w-32'>
        {event.hash.slice(0, 8)}
      </td>
    </tr>
  );
}
