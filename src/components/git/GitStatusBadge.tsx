import { Badge } from '@/components/ui/badge';
import { GIT_STATUS_CONFIG } from '@/lib/colors';
import type { GitStatusKind } from '@/lib/tauri-api';

interface GitStatusBadgeProps {
  status: GitStatusKind;
}

// Renders a compact letter badge for a git file status (M, A, D, R, ?, C).
export function GitStatusBadge({ status }: GitStatusBadgeProps) {
  if (status === 'unchanged') return null;
  const config = GIT_STATUS_CONFIG[status];
  return (
    <Badge
      variant="outline"
      className={`h-4 px-1 text-[9px] font-mono leading-none ${config.className}`}
    >
      {config.label}
    </Badge>
  );
}
