import { cn } from '@/lib/utils';
import type { ContractScope } from '@/types/contracts';

type ScopeTagProps =
  | { scope: ContractScope; type?: never; count?: never; label?: never; className?: string }
  | { scope?: never; type: 'endpoints'; count: number; label?: never; className?: string }
  | { scope?: never; type: 'policy'; count?: never; label: string; className?: string }
  | { scope?: never; type: 'sla'; count?: never; label: string | null; className?: string };

export function ScopeTag(props: ScopeTagProps & { className?: string }) {
  const { className } = props;
  let prefix: string;
  let value: string;

  if (props.scope !== undefined) {
    const s = props.scope;
    if (s.type === 'collection') {
      prefix = 'Scope';
      value = 'Entire collection';
    } else if (s.type === 'folder') {
      prefix = 'Folder';
      value = s.rel_path;
    } else {
      // type === 'request'
      prefix = 'Request';
      value = s.rel_path;
    }
  } else if (props.type === 'endpoints') {
    prefix = 'Endpoints';
    value = String(props.count);
  } else if (props.type === 'policy') {
    prefix = 'Policy';
    value = props.label;
  } else if (props.type === 'sla') {
    prefix = 'SLA';
    value = props.label ? `${props.label}%` : '—';
  } else {
    return null;
  }

  return (
    <span
      className={cn(
        'inline-flex items-center gap-[5px] px-2 py-0.5',
        'font-mono text-[11px] text-foreground',
        'bg-muted border border-border rounded-[4px]',
        className,
      )}
    >
      <span className='font-sans text-muted-foreground font-medium not-italic text-[10px]'>
        {prefix}
      </span>
      {value}
    </span>
  );
}
