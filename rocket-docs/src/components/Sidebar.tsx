import { Search } from 'lucide-react';
import { useMemo, useState } from 'react';
import { NavLink } from 'react-router-dom';

import { sidebar } from '@/lib/sidebar';
import { cn } from '@/lib/utils';

export function Sidebar({ onNavigate }: { onNavigate?: () => void }) {
  const [filter, setFilter] = useState('');

  const groups = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return sidebar;
    return sidebar
      .map((group) => ({
        ...group,
        items: group.items.filter((item) => item.title.toLowerCase().includes(needle)),
      }))
      .filter((group) => group.items.length > 0);
  }, [filter]);

  return (
    <nav aria-label="Guide" className="flex h-full flex-col gap-5 overflow-y-auto px-4 py-6">
      <div className="relative">
        <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
        <input
          type="search"
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
          placeholder="Filter pages…"
          aria-label="Filter sidebar pages by title"
          className="w-full rounded-md border border-border bg-card py-1.5 pr-3 pl-8 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
        />
      </div>

      {groups.map((group) => (
        <div key={group.title}>
          <p className="font-heading text-[11px] tracking-[0.14em] text-muted-foreground uppercase">
            {group.title}
          </p>
          <ul className="mt-2 space-y-0.5">
            {group.items.map((item) => (
              <li key={item.slug}>
                <NavLink
                  to={`/guide/${item.slug}`}
                  onClick={onNavigate}
                  className={({ isActive }) =>
                    cn(
                      'block rounded-md px-2.5 py-1.5 text-sm transition-colors',
                      isActive
                        ? 'bg-primary/10 font-medium text-primary'
                        : 'text-muted-foreground hover:bg-accent hover:text-foreground',
                    )
                  }
                >
                  {item.title}
                </NavLink>
              </li>
            ))}
          </ul>
        </div>
      ))}

      {groups.length === 0 ? <p className="text-sm text-muted-foreground">No pages match "{filter}".</p> : null}
    </nav>
  );
}
