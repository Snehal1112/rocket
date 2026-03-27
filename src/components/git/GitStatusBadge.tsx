import { Badge } from '@/components/ui/badge';
import type { GitStatusKind } from '@/lib/tauri-api';

const statusConfig: Record<GitStatusKind, { label: string; className: string }> = {
  modified:  { label: 'M', className: 'bg-amber-500/20 text-amber-400 border-amber-500/30' },
  added:     { label: 'A', className: 'bg-green-500/20 text-green-400 border-green-500/30' },
  deleted:   { label: 'D', className: 'bg-red-500/20 text-red-400 border-red-500/30' },
  renamed:   { label: 'R', className: 'bg-blue-500/20 text-blue-400 border-blue-500/30' },
  untracked: { label: '?', className: 'bg-gray-500/20 text-gray-400 border-gray-500/30' },
  conflicted: { label: 'C', className: 'bg-red-700/20 text-red-300 border-red-700/30' },
  unchanged: { label: '', className: '' },
};

interface GitStatusBadgeProps {
  status: GitStatusKind;
}

// Renders a compact letter badge for a git file status (M, A, D, R, ?, C).
export function GitStatusBadge({ status }: GitStatusBadgeProps) {
  if (status === 'unchanged') return null;
  const config = statusConfig[status];
  return (
    <Badge
      variant="outline"
      className={`h-4 px-1 text-[9px] font-mono leading-none ${config.className}`}
    >
      {config.label}
    </Badge>
  );
}
