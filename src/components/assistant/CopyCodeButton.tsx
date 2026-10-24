import { Check, Copy } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';

/** Copies a code block to the clipboard and shows a check for a moment. */
export function CopyCodeButton({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      // The clipboard can be unavailable. The code stays selectable.
    }
  };

  const Icon = copied ? Check : Copy;
  return (
    <Button
      variant='ghost'
      size='icon'
      className='size-6 text-muted-foreground'
      aria-label={copied ? 'Copied' : 'Copy code'}
      title={copied ? 'Copied' : 'Copy code'}
      onClick={() => void handleCopy()}
    >
      <Icon className='h-3.5 w-3.5' aria-hidden='true' />
    </Button>
  );
}
