import { useEffect } from 'react';
import { Link } from 'react-router-dom';

import { Nav } from '@/components/Nav';
import { buttonVariants } from '@/components/ui/button-variants';
import { cn } from '@/lib/utils';

export function NotFound() {
  useEffect(() => {
    document.title = '404 — Rocket Manual';
  }, []);

  return (
    <div className="flex min-h-screen flex-col">
      <Nav />
      <main className="flex flex-1 flex-col items-center justify-center px-5 text-center">
        <p className="font-heading text-sm tracking-[0.2em] text-muted-foreground uppercase">404</p>
        <h1 className="mt-3 text-2xl font-semibold">Page not found</h1>
        <p className="mt-2 max-w-md text-muted-foreground">
          This page doesn't exist, or it moved. Try the guide from the start.
        </p>
        <Link to="/guide/getting-started" className={cn(buttonVariants({ variant: 'primary' }), 'mt-6')}>
          Go to Getting started
        </Link>
      </main>
    </div>
  );
}
