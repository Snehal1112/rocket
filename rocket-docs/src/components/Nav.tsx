import { Menu, X } from 'lucide-react';
import { Link } from 'react-router-dom';

import { GithubIcon } from '@/components/GithubIcon';
import { ThemeToggle } from '@/components/ThemeToggle';
import { Button } from '@/components/ui/button';
import { buttonVariants } from '@/components/ui/button-variants';
import { links } from '@/lib/sidebar';
import { cn } from '@/lib/utils';

interface NavProps {
  sidebarOpen?: boolean;
  onToggleSidebar?: () => void;
}

export function Nav({ sidebarOpen, onToggleSidebar }: NavProps) {
  return (
    <header className="sticky top-0 z-40 w-full border-b border-border bg-background/85 backdrop-blur-md">
      <div className="flex h-16 w-full items-center gap-3 px-4 sm:px-6">
        {onToggleSidebar ? (
          <Button
            variant="ghost"
            size="icon"
            className="lg:hidden"
            aria-label={sidebarOpen ? 'Close the sidebar' : 'Open the sidebar'}
            aria-expanded={sidebarOpen}
            onClick={onToggleSidebar}
          >
            {sidebarOpen ? <X /> : <Menu />}
          </Button>
        ) : null}

        <Link to="/" className="flex shrink-0 items-center gap-2.5 rounded-md">
          <img src="/rocket.png" alt="" aria-hidden="true" className="size-7" />
          <span className="font-heading text-base font-semibold tracking-tight">Rocket</span>
          <span className="font-heading text-base font-semibold tracking-tight text-muted-foreground">
            Manual
          </span>
        </Link>

        <nav aria-label="Sections" className="ml-4 hidden items-center gap-1 lg:flex">
          <Link
            to="/guide/getting-started"
            className="rounded-md px-3 py-2 font-heading text-[13px] tracking-tight text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          >
            Guide
          </Link>
        </nav>

        <div className="ml-auto flex items-center gap-1">
          <a
            href={links.site}
            target="_blank"
            rel="noreferrer"
            className={cn(
              buttonVariants({ variant: 'ghost', size: 'sm' }),
              'hidden font-heading text-[13px] tracking-tight sm:inline-flex',
            )}
          >
            Rocket
          </a>
          <a
            href={links.github}
            target="_blank"
            rel="noreferrer"
            aria-label="Rocket on GitHub"
            className={cn(buttonVariants({ variant: 'ghost', size: 'icon' }))}
          >
            <GithubIcon />
          </a>
          <ThemeToggle />
        </div>
      </div>
    </header>
  );
}
