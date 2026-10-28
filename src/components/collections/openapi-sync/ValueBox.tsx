/** A titled value as plain text. A missing value reads "(none)". */
export function ValueBox({ title, value }: { title: string; value?: string | null }) {
  return (
    <div className='min-w-0'>
      <p className='text-xs font-medium text-muted-foreground'>{title}</p>
      <pre className='max-h-48 overflow-auto whitespace-pre-wrap break-all rounded-sm border border-border bg-muted/40 p-2 font-mono text-xs'>
        {value == null ? '(none)' : value}
      </pre>
    </div>
  );
}
