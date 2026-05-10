interface ContractsGroupHeaderProps {
  label: string;
  count: number;
}

export function ContractsGroupHeader({ label, count }: ContractsGroupHeaderProps) {
  return (
    <div className='flex items-center gap-2 px-1 py-2 mb-1 mt-4 first:mt-0'>
      <span className='text-xs font-semibold text-muted-foreground uppercase tracking-[0.06em]'>
        {label}
      </span>
      <span className='text-xs text-muted-foreground/60 tabular-nums'>{count}</span>
      <div className='flex-1 h-[1px] bg-border/50 ml-1' />
    </div>
  );
}
