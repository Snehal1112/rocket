import type { ReactNode } from 'react';
import { Label } from '@/components/ui/label';

export interface FieldA11yProps {
  id: string;
  'aria-invalid': boolean;
  'aria-describedby': string | undefined;
}

export interface FormFieldProps {
  id: string;
  label: ReactNode;
  optionalHint?: string;
  /** True when the label names a group of controls, such as a radio set or a row list. */
  group?: boolean;
  help: ReactNode;
  error?: string | null;
  children: (a11y: FieldA11yProps) => ReactNode;
}

/**
 * Label, control, always-visible help and an optional error.
 * The control comes from the render prop so it gets the id and aria wiring.
 * In group mode the wrapper is a labelled group and the label is not a <label>.
 */
export function FormField({
  id,
  label,
  optionalHint,
  group = false,
  help,
  error,
  children,
}: FormFieldProps) {
  const helpId = `${id}-help`;
  const errorId = `${id}-error`;
  const labelId = `${id}-label`;
  const hasError = Boolean(error);
  const describedBy = hasError ? `${helpId} ${errorId}` : helpId;

  const labelContent = (
    <>
      {label}
      {optionalHint && (
        <span className='ml-1.5 text-xs font-normal text-muted-foreground'>({optionalHint})</span>
      )}
    </>
  );

  return (
    <div className='space-y-1.5' {...(group ? { role: 'group', 'aria-labelledby': labelId } : {})}>
      {group ? (
        <span id={labelId} className='block text-sm font-medium leading-none'>
          {labelContent}
        </span>
      ) : (
        <Label htmlFor={id}>{labelContent}</Label>
      )}
      {children({ id, 'aria-invalid': hasError, 'aria-describedby': describedBy })}
      <p id={helpId} className='break-words text-xs leading-relaxed text-muted-foreground'>
        {help}
      </p>
      {hasError && (
        <p id={errorId} role='alert' className='break-words text-xs text-destructive'>
          {error}
        </p>
      )}
    </div>
  );
}
