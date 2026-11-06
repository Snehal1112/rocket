import { toast } from 'sonner';

// The backend sends errors as their Display text, so the type prefix is dropped.
export function errorText(err: unknown, fallback = 'Something went wrong. Try again.'): string {
  const raw = err instanceof Error ? err.message : typeof err === 'string' ? err : '';
  const text = raw.replace(/^(Invalid input|Internal error): /, '').trim();
  return text || fallback;
}

// Shows the cleaned error text as an error toast. The optional prefix goes in front of it.
export function toastError(err: unknown, prefix?: string): void {
  const text = errorText(err);
  toast.error(prefix ? `${prefix} ${text}` : text);
}
