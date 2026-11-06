interface NetworkIdentityProps {
  name: string;
  shortId: string;
  /** Text after the short id, for example the pairing code of a request. */
  extra?: string;
}

/** The name of a node with its short id below it. The short id is the only id the UI shows. */
export function NetworkIdentity({ name, shortId, extra }: NetworkIdentityProps) {
  return (
    <div className='min-w-0 flex-1'>
      <p className='truncate text-xs font-medium'>{name}</p>
      <p className='font-mono text-2xs text-muted-foreground'>
        {extra ? `${shortId} · ${extra}` : shortId}
      </p>
    </div>
  );
}
