import { AlertCircle, X } from 'lucide-react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';

interface GitErrorBannerProps {
  message: string;
  onDismiss?: () => void;
  className?: string;
}

// Shared error banner for Git panel views — replaces the near-identical
// inline alert markup previously duplicated across ConflictResolver,
// GitLandingPanel, and GitStashSection (see
// docs/reports/git-integration-review/02-frontend-architecture.md, dup-obs #2).
export function GitErrorBanner({ message, onDismiss, className }: GitErrorBannerProps) {
  return (
    <Alert variant='destructive' className={className}>
      <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
      <span className='flex-1 wrap-break-word'>{message}</span>
      {onDismiss && (
        <Button
          variant='ghost'
          size='icon'
          className='h-4 w-4 shrink-0'
          onClick={onDismiss}
          aria-label='Dismiss error'
        >
          <X className='h-3 w-3' />
        </Button>
      )}
    </Alert>
  );
}
