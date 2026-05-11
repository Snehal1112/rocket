import { useMemo } from 'react';
import { useDrawerStore } from '@/stores/contracts/drawerSlice';
import type { ChangelogEntry, Contract } from '@/types/contracts';

export interface DayGroup {
  day: string;
  label: string;
  items: ChangelogEntry[];
}

export function useChangelogEntries(contract: Contract): DayGroup[] {
  const filters = useDrawerStore((s) => s.filters);

  return useMemo(() => {
    let entries = [...contract.changelog];

    if (filters.sinceSigned) {
      entries = entries.filter((e) => e.at >= contract.effectiveAt);
    }
    if (filters.breakingOnly) entries = entries.filter((e) => e.isBreaking);
    if (filters.kinds.length > 0) entries = entries.filter((e) => filters.kinds.includes(e.kind));
    if (filters.search.trim()) {
      const q = filters.search.toLowerCase();
      entries = entries.filter(
        (e) =>
          e.summary.toLowerCase().includes(q) ||
          e.requestPath?.toLowerCase().includes(q) ||
          e.authorName?.toLowerCase().includes(q),
      );
    }

    entries.sort((a, b) => b.at.localeCompare(a.at));

    return groupByDay(entries);
  }, [contract.changelog, contract.effectiveAt, filters]);
}

function groupByDay(entries: ChangelogEntry[]): DayGroup[] {
  const groups: Record<string, ChangelogEntry[]> = {};
  entries.forEach((e) => {
    const day = new Date(e.at).toLocaleDateString('en-CA');
    if (!groups[day]) groups[day] = [];
    groups[day].push(e);
  });
  return Object.entries(groups).map(([day, items]) => ({ day, label: formatDayLabel(day), items }));
}

function formatDayLabel(day: string): string {
  const today = new Date().toLocaleDateString('en-CA');
  const yest = new Date();
  yest.setDate(yest.getDate() - 1);
  const yesterday = yest.toLocaleDateString('en-CA');
  const formatted = new Date(`${day}T12:00:00`).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
  });
  if (day === today) return `Today · ${formatted}`;
  if (day === yesterday) return `Yesterday · ${formatted}`;
  return formatted;
}
