/** Mono bordered version badge shown inline with contract title. */
export function VersionTag({ version, isDraft }: { version: string; isDraft?: boolean }) {
  return (
    <span className='font-mono text-xs border border-border rounded px-1.5 py-0.5 text-muted-foreground font-normal shrink-0'>
      {version}
      {isDraft && <span className='ml-1 font-sans text-[10px] not-italic'>draft</span>}
    </span>
  );
}
