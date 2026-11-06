import { GithubIcon } from '@/components/GithubIcon';
import { links, navLinks } from '@/lib/site';

const resources = [
  { href: links.docs, label: 'Docs' },
  { href: links.github, label: 'GitHub' },
  { href: links.benchmarks, label: 'Benchmarks' },
  { href: links.license, label: 'MIT License' },
  { href: `mailto:${links.contactEmail}`, label: 'Contact' },
];

// Read once at load. A static page has no reason to re-read the clock per render.
const year = new Date().getFullYear();

export function Footer() {
  return (
    <footer className="border-t border-border bg-background">
      <div className="mx-auto w-full max-w-6xl px-5 py-12 sm:px-8 sm:py-14">
        <div className="flex flex-col gap-10 sm:flex-row sm:justify-between">
          <div className="max-w-xs">
            <a href="#top" className="flex items-center gap-2.5">
              <img src="/rocket.png" alt="" aria-hidden="true" className="size-7" />
              <span className="font-heading text-base font-semibold tracking-tight">Rocket</span>
            </a>
            <p className="mt-4 text-sm leading-relaxed text-muted-foreground">
              A native desktop API client for HTTP, GraphQL, gRPC and WebSocket. Local files, real
              git, and an agent that asks first.
            </p>
          </div>

          <div className="grid grid-cols-2 gap-10 sm:gap-16">
            <nav aria-label="Page sections">
              <h2 className="font-heading text-[11px] tracking-[0.18em] text-muted-foreground uppercase">
                On this page
              </h2>
              <ul className="mt-4 space-y-2.5">
                {navLinks.map((link) => (
                  <li key={link.href}>
                    <a
                      href={link.href}
                      className="text-sm text-muted-foreground transition-colors hover:text-foreground"
                    >
                      {link.label}
                    </a>
                  </li>
                ))}
              </ul>
            </nav>

            <nav aria-label="Project resources">
              <h2 className="font-heading text-[11px] tracking-[0.18em] text-muted-foreground uppercase">
                Project
              </h2>
              <ul className="mt-4 space-y-2.5">
                {resources.map((resource) => (
                  <li key={resource.label}>
                    <a
                      href={resource.href}
                      target="_blank"
                      rel="noreferrer"
                      className="text-sm text-muted-foreground transition-colors hover:text-foreground"
                    >
                      {resource.label}
                    </a>
                  </li>
                ))}
              </ul>
            </nav>
          </div>
        </div>

        <div className="mt-12 flex flex-col-reverse items-start gap-4 border-t border-border pt-6 sm:flex-row sm:items-center sm:justify-between">
          <p className="font-heading text-[11px] tracking-wide text-muted-foreground">
            © {year} Numericlabs.io · MIT licensed · No cookies, no analytics on this
            page.
          </p>
          <a
            href={links.github}
            target="_blank"
            rel="noreferrer"
            aria-label="Rocket on GitHub"
            className="rounded-md p-2 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <GithubIcon />
          </a>
        </div>
      </div>
    </footer>
  );
}
