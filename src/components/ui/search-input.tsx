import { Search } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

export interface SearchInputProps {
  value: string;
  onChange: (value: string) => void;
  /** The accessible name of the box. */
  label: string;
  placeholder?: string;
  /** Goes on the wrapper, for example a width. */
  className?: string;
}

/** A compact search box with a magnifier icon. It hands the text to `onChange`. */
export function SearchInput({ value, onChange, label, placeholder, className }: SearchInputProps) {
  return (
    <div className={cn('relative', className)}>
      <Search
        className='pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground'
        aria-hidden='true'
      />
      <Input
        role='searchbox'
        aria-label={label}
        placeholder={placeholder}
        className='h-8 pl-7 text-sm'
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}
