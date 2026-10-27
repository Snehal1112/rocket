import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { FileCode } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';

interface SpecSourceFieldProps {
  /** A link, or the full path of the chosen file. */
  value: string;
  onChange: (value: string) => void;
  /** The accessible name of the link input. */
  label: string;
  disabled?: boolean;
  placeholder?: string;
}

/** A spec link, or a spec file chosen with the system picker. */
export function SpecSourceField({
  value,
  onChange,
  label,
  disabled,
  placeholder = 'https://github.com/owner/repo/blob/main/openapi.yaml',
}: SpecSourceFieldProps) {
  const [mode, setMode] = useState<'link' | 'file'>('link');

  const switchTo = (next: 'link' | 'file') => {
    if (next === mode) return;
    setMode(next);
    // A link and a path do not mean the same thing in the other mode.
    onChange('');
  };

  const choose = async () => {
    const picked = await openFilePicker({
      multiple: false,
      directory: false,
      filters: [{ name: 'OpenAPI', extensions: ['json', 'yaml', 'yml'] }],
    });
    if (typeof picked === 'string' && picked) onChange(picked);
  };

  return (
    <div className='space-y-2'>
      <div className='flex w-fit gap-1 rounded-md border border-border p-0.5'>
        <Button
          variant={mode === 'link' ? 'secondary' : 'ghost'}
          size='sm'
          className='h-7 px-3 text-xs'
          disabled={disabled}
          onClick={() => switchTo('link')}
        >
          Link
        </Button>
        <Button
          variant={mode === 'file' ? 'secondary' : 'ghost'}
          size='sm'
          className='h-7 px-3 text-xs'
          disabled={disabled}
          onClick={() => switchTo('file')}
        >
          File
        </Button>
      </div>
      {mode === 'link' ? (
        <Input
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          aria-label={label}
          disabled={disabled}
        />
      ) : (
        <div className='flex items-center gap-2'>
          <Button variant='outline' size='sm' disabled={disabled} onClick={() => void choose()}>
            <FileCode aria-hidden='true' className='mr-1.5 h-3.5 w-3.5' />
            Choose spec file
          </Button>
          {value && <p className='min-w-0 break-all text-xs text-muted-foreground'>{value}</p>}
        </div>
      )}
    </div>
  );
}
