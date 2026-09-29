import { ChevronDown, ChevronUp } from 'lucide-react';
import * as React from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

type NumberInputProps = Omit<React.ComponentProps<'input'>, 'type'> & {
  // Classes for the outer wrapper, such as a fixed width.
  containerClassName?: string;
};

// Numeric input with a themed stepper in place of the native browser spinner.
function NumberInput({ className, containerClassName, disabled, ...props }: NumberInputProps) {
  const inputRef = React.useRef<HTMLInputElement>(null);

  // Step through the native API so min, max and step stay enforced.
  // The input event lets React fire the caller's onChange as if typed.
  const stepBy = (direction: 1 | -1) => {
    const input = inputRef.current;
    if (!input) return;
    if (direction === 1) input.stepUp();
    else input.stepDown();
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };

  return (
    <div data-slot='number-input' className={cn('relative w-full', containerClassName)}>
      <Input
        ref={inputRef}
        type='number'
        disabled={disabled}
        className={cn(
          'pr-7 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none',
          className,
        )}
        {...props}
      />
      <div className='absolute inset-y-px right-px flex w-6 flex-col overflow-hidden rounded-r-[5px] border-l border-input'>
        <Button
          type='button'
          variant='ghost'
          tabIndex={-1}
          aria-label='Increment'
          disabled={disabled}
          onClick={() => stepBy(1)}
          className='h-1/2 min-h-0 w-full rounded-none p-0 text-muted-foreground hover:text-foreground [&_svg:not([class*="size-"])]:size-3'
        >
          <ChevronUp />
        </Button>
        <Button
          type='button'
          variant='ghost'
          tabIndex={-1}
          aria-label='Decrement'
          disabled={disabled}
          onClick={() => stepBy(-1)}
          className='h-1/2 min-h-0 w-full rounded-none border-t border-input p-0 text-muted-foreground hover:text-foreground [&_svg:not([class*="size-"])]:size-3'
        >
          <ChevronDown />
        </Button>
      </div>
    </div>
  );
}

export { NumberInput };
