import { GIT_STATUS_CONFIG } from '@/lib/colors';
import type { GitStatusKind } from '@/lib/tauri-api';

interface GitStatusBadgeProps {
  status: GitStatusKind;
}

// Renders a colored status letter (M / A / D / R / U / C) inline — no badge border.
export function GitStatusBadge({ status }: GitStatusBadgeProps) {
  if (status === 'unchanged') return null;
  const config = GIT_STATUS_CONFIG[status];
  return (
    <span
      className={`shrink-0 w-3.5 text-center font-mono text-[11px] font-bold leading-none ${config.className}`}
    >
      {config.label}
    </span>
  );
}
