import { Pencil } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import { normalizeMethod, STANDARD_METHODS, withCurrentMethod } from '@/lib/method-options';
import { cn } from '@/lib/utils';

interface MethodSelectProps {
  value: string;
  onChange: (method: string) => void;
}

// Method picker for the URL bar: the standard methods plus a custom-method field.
export function MethodSelect({ value, onChange }: MethodSelectProps) {
  const [customOpen, setCustomOpen] = useState(false);
  const [customText, setCustomText] = useState('');
  const [invalid, setInvalid] = useState(false);

  const closeCustom = () => {
    setCustomOpen(false);
    setCustomText('');
    setInvalid(false);
  };

  const commitCustom = () => {
    const method = normalizeMethod(customText);
    if (!method) {
      setInvalid(true);
      return;
    }
    onChange(method);
    closeCustom();
  };

  return (
    <div className='flex items-center gap-1'>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger className={cn('h-8 w-28 text-sm font-semibold', METHOD_TEXT_COLOR[value])}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {withCurrentMethod(STANDARD_METHODS, value).map((m) => (
            <SelectItem
              key={m}
              value={m}
              className={cn('text-sm font-semibold', METHOD_TEXT_COLOR[m])}
            >
              {m}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {customOpen ? (
        <Input
          autoFocus
          aria-label='Custom HTTP method'
          aria-invalid={invalid}
          placeholder='PURGE'
          value={customText}
          onChange={(e) => {
            setCustomText(e.target.value);
            setInvalid(false);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commitCustom();
            if (e.key === 'Escape') closeCustom();
          }}
          className={cn('h-8 w-28 text-xs font-mono', invalid && 'border-destructive')}
        />
      ) : (
        <Button
          variant='ghost'
          size='icon'
          className='h-8 w-8 text-muted-foreground'
          aria-label='Custom method'
          title='Use a custom HTTP method'
          onClick={() => setCustomOpen(true)}
        >
          <Pencil className='h-3.5 w-3.5' />
        </Button>
      )}
    </div>
  );
}
