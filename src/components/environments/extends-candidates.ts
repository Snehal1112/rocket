// src/components/environments/extends-candidates.ts

import type { Environment } from '@/lib/tauri-api';

// Names that can be chosen as the parent of `currentName`.
// Excludes the environment itself and every descendant, so no cycle can form.
export function extendsCandidates(envs: Environment[], currentName: string): string[] {
  const parentOf = new Map<string, string | undefined>();
  for (const e of envs) parentOf.set(e.name, e.extends || undefined);

  const reachesCurrent = (start: string): boolean => {
    const seen = new Set<string>();
    let cursor: string | undefined = start;
    while (cursor !== undefined && !seen.has(cursor)) {
      if (cursor === currentName) return true;
      seen.add(cursor);
      cursor = parentOf.get(cursor);
    }
    return false;
  };

  return envs
    .map((e) => e.name)
    .filter((name) => !reachesCurrent(name))
    .sort((a, b) => a.localeCompare(b));
}
