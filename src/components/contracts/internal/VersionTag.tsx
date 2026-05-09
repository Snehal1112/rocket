/** Mono bordered version badge shown inline with contract title. */
export function VersionTag({ version }: { version: string }) {
  return (
    <span className='font-mono text-xs border border-border rounded px-1.5 py-0.5 text-muted-foreground font-normal shrink-0'>
      {version}
    </span>
  );
}
