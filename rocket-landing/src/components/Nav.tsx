import { Download, Menu, X } from 'lucide-react';
import { useEffect, useState } from 'react';

import { GithubIcon } from '@/components/GithubIcon';
import { ThemeToggle } from '@/components/ThemeToggle';
import { Button } from '@/components/ui/button';
import { buttonVariants } from '@/components/ui/button-variants';
import { links, navLinks, sections } from '@/lib/site';
import { cn } from '@/lib/utils';

export function Nav() {
  const [scrolled, setScrolled] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, []);

  // The mobile panel takes over the viewport, so stop the page behind it scrolling.
  useEffect(() => {
    document.body.style.overflow = open ? 'hidden' : '';
    return () => {
      document.body.style.overflow = '';
    };
  }, [open]);

  // The panel is hidden from `lg` up. Close it on the way past, or the scroll
  // lock outlives the thing that set it.
  useEffect(() => {
    if (!open) return;
    const wide = window.matchMedia('(min-width: 64rem)');
    const close = () => {
      if (wide.matches) setOpen(false);
    };
    close();
    wide.addEventListener('change', close);
    return () => wide.removeEventListener('change', close);
  }, [open]);

  // Escape closes the panel, the way every other overlay does.
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open]);

  return (
    <header
      className={cn(
        'sticky top-0 z-50 w-full transition-colors duration-200',
        scrolled || open
          ? 'border-b border-border bg-background/85 backdrop-blur-md'
          : 'border-b border-transparent bg-background/0',
      )}
    >
      <div className="flex h-16 w-full items-center gap-4 px-5 sm:px-8">
        <a
          href="#top"
          className="flex shrink-0 items-center gap-2.5 rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
        >
          <img src="/rocket.png" alt="" aria-hidden="true" className="size-7" />
          <span className="font-heading text-base font-semibold tracking-tight">Rocket</span>
        </a>

        <nav aria-label="Primary" className="ml-4 hidden items-center gap-1 lg:flex">
          {navLinks.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="rounded-md px-3 py-2 font-heading text-[13px] tracking-tight text-muted-foreground transition-colors duration-150 hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {link.label}
            </a>
          ))}
          <a
            href={links.docs}
            target="_blank"
            rel="noreferrer"
            className="rounded-md px-3 py-2 font-heading text-[13px] tracking-tight text-muted-foreground transition-colors duration-150 hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Docs
          </a>
          <a
            href={`mailto:${links.contactEmail}`}
            target="_blank"
            rel="noreferrer"
            className="rounded-md px-3 py-2 font-heading text-[13px] tracking-tight text-muted-foreground transition-colors duration-150 hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Contact Us
          </a>
        </nav>

        <div className="ml-auto flex items-center gap-1 sm:gap-2">
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
          <a
            href={`#${sections.download}`}
            className={cn(buttonVariants({ variant: 'primary', size: 'sm' }), 'hidden sm:inline-flex')}
          >
            <Download />
            Download
          </a>
          <Button
            variant="ghost"
            size="icon"
            className="lg:hidden"
            aria-label={open ? 'Close the menu' : 'Open the menu'}
            aria-expanded={open}
            aria-controls="mobile-nav"
            onClick={() => setOpen((value) => !value)}
          >
            {open ? <X /> : <Menu />}
          </Button>
        </div>
      </div>

      {open ? (
        <nav
          id="mobile-nav"
          aria-label="Primary"
          className="border-t border-border bg-background px-5 pt-3 pb-6 sm:px-8 lg:hidden"
        >
          <ul className="flex flex-col">
            {navLinks.map((link) => (
              <li key={link.href}>
                <a
                  href={link.href}
                  onClick={() => setOpen(false)}
                  className="block rounded-md px-2 py-3 font-heading text-sm tracking-tight text-foreground transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  {link.label}
                </a>
              </li>
            ))}
            <li>
              <a
                href={links.docs}
                target="_blank"
                rel="noreferrer"
                className="block rounded-md px-2 py-3 font-heading text-sm tracking-tight text-foreground transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Docs
              </a>
            </li>
            <li>
              <a
                href={`mailto:${links.contactEmail}`}
                target="_blank"
                rel="noreferrer"
                onClick={() => setOpen(false)}
                className="block rounded-md px-2 py-3 font-heading text-sm tracking-tight text-foreground transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Contact Us
              </a>
            </li>
          </ul>
          <a
            href={`#${sections.download}`}
            onClick={() => setOpen(false)}
            className={cn(buttonVariants({ variant: 'primary', size: 'md' }), 'mt-4 w-full')}
          >
            <Download />
            Download Rocket
          </a>
        </nav>
      ) : null}
    </header>
  );
}
