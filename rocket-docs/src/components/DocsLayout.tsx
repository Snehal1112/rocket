import { type ReactNode, useEffect, useState } from 'react';

import { Nav } from '@/components/Nav';
import { Sidebar } from '@/components/Sidebar';
import { cn } from '@/lib/utils';

export function DocsLayout({ outline, children }: { outline?: ReactNode; children: ReactNode }) {
  const [open, setOpen] = useState(false);

  // Lock page scroll behind the mobile drawer while it's open.
  useEffect(() => {
    document.body.style.overflow = open ? 'hidden' : '';
    return () => {
      document.body.style.overflow = '';
    };
  }, [open]);

  return (
    <div className="flex min-h-screen flex-col">
      <Nav sidebarOpen={open} onToggleSidebar={() => setOpen((v) => !v)} />

      <div className="mx-auto flex w-full max-w-7xl flex-1 items-start">
        <aside className="sticky top-16 hidden h-[calc(100vh-4rem)] w-64 shrink-0 border-r border-border lg:block">
          <Sidebar />
        </aside>

        {open ? (
          <>
            <button
              type="button"
              aria-label="Close the sidebar"
              className="fixed inset-0 z-30 bg-black/40 lg:hidden"
              onClick={() => setOpen(false)}
            />
            <aside className="fixed inset-y-0 left-0 z-40 w-72 border-r border-border bg-background lg:hidden">
              <div className="pt-16">
                <Sidebar onNavigate={() => setOpen(false)} />
              </div>
            </aside>
          </>
        ) : null}

        <main className={cn('min-w-0 flex-1 px-5 py-10 sm:px-8 lg:px-10', outline && 'xl:pr-0')}>
          <div className="mx-auto flex w-full max-w-4xl gap-10 xl:max-w-none xl:mr-10">
            <div className="min-w-0 flex-1">{children}</div>
            {outline}
          </div>
        </main>
      </div>
    </div>
  );
}
