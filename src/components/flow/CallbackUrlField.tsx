import { Check, Copy } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { copyTextAsync } from '@/lib/clipboard';

const COPIED_MS = 1500;

// The live callback URL of a running flow's Wait node, with a copy button. The
// URL holds a token and works only while the run is active.
export function CallbackUrlField({ url }: { url: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const copy = () => {
    // Started in the click handler, so WebKit keeps the user activation.
    copyTextAsync(Promise.resolve(url)).then(
      () => {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), COPIED_MS);
      },
      (err) => console.warn('Copy failed', err),
    );
  };

  return (
    <div data-testid='callback-url' className='nodrag nokey space-y-0.5'>
      <div className='flex items-center gap-1'>
        <code className='min-w-0 truncate font-mono text-[11px]'>{url}</code>
        <Button
          type='button'
          variant='ghost'
          size='icon'
          className='h-5 w-5 shrink-0'
          aria-label='Copy callback URL'
          onClick={copy}
        >
          {copied ? <Check className='h-3 w-3' /> : <Copy className='h-3 w-3' />}
        </Button>
      </div>
      <p className='text-[10px] text-muted-foreground'>Valid while the run is active.</p>
    </div>
  );
}
