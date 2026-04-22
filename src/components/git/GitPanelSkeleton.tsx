export function GitPanelSkeleton() {
  return (
    <div className='flex flex-col h-full' aria-hidden='true' aria-busy='true'>
      <div className='flex-1 flex overflow-hidden'>
        {/* LEFT PANEL */}
        <div className='w-[320px] shrink-0 border-r border-border/70 flex flex-col overflow-hidden'>
          {/* Header: collection name + branch selector placeholder */}
          <div className='flex items-center gap-2 px-3 py-2.5 border-b border-border/70 shrink-0'>
            <div className='h-3.5 w-3.5 rounded-sm bg-muted motion-safe:animate-pulse' />
            <div className='h-3 w-28 rounded-sm bg-muted motion-safe:animate-pulse flex-1' />
            <div className='h-6 w-24 rounded-md bg-muted motion-safe:animate-pulse' />
          </div>

          {/* Commit form placeholder */}
          <div className='shrink-0 px-3 pt-2.5 pb-2 border-b border-border/70 space-y-2'>
            <div className='h-16 w-full rounded-md bg-muted/60 motion-safe:animate-pulse' />
            <div className='h-7 w-full rounded-md bg-muted/60 motion-safe:animate-pulse' />
          </div>

          {/* File list placeholder — 5 rows */}
          <div className='flex-1 px-3 py-2 space-y-1 overflow-hidden'>
            {[80, 60, 72, 55, 68].map((w) => (
              <div key={w} className='flex items-center gap-2 py-1'>
                <div className='h-3 w-3 rounded-sm bg-muted motion-safe:animate-pulse shrink-0' />
                <div
                  className='h-2.5 rounded-sm bg-muted/70 motion-safe:animate-pulse'
                  style={{ width: `${w}%` }}
                />
              </div>
            ))}
          </div>

          {/* Links section placeholder */}
          <div className='shrink-0 border-t border-border/70 px-3 py-2 space-y-1'>
            {[1, 2, 3].map((n) => (
              <div
                key={n}
                className='h-8 w-full rounded-md bg-muted/50 motion-safe:animate-pulse'
              />
            ))}
          </div>
        </div>

        {/* Resize handle */}
        <div className='w-1.5 shrink-0 bg-border/35' />

        {/* RIGHT PANEL — landing card placeholder */}
        <div className='flex-1 overflow-hidden flex flex-col p-4'>
          <div className='rounded-lg border border-border/70 bg-card p-4 space-y-4'>
            {/* Branch / sync status row */}
            <div className='flex items-center gap-2'>
              <div className='h-4 w-4 rounded-full bg-muted motion-safe:animate-pulse' />
              <div className='h-3 w-32 rounded-sm bg-muted motion-safe:animate-pulse' />
              <div className='ml-auto flex gap-1.5'>
                <div className='h-5 w-10 rounded-full bg-muted/60 motion-safe:animate-pulse' />
                <div className='h-5 w-10 rounded-full bg-muted/60 motion-safe:animate-pulse' />
              </div>
            </div>

            {/* Action buttons row */}
            <div className='flex gap-2'>
              {[1, 2, 3].map((n) => (
                <div
                  key={n}
                  className='h-8 flex-1 rounded-md bg-muted/60 motion-safe:animate-pulse'
                />
              ))}
            </div>

            {/* Divider */}
            <div className='h-px w-full bg-border/50' />

            {/* Sync timestamp placeholder */}
            <div className='h-2.5 w-40 rounded-sm bg-muted/50 motion-safe:animate-pulse' />
          </div>
        </div>
      </div>
    </div>
  );
}
