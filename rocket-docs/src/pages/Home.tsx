import { FolderTree, KeyRound, Gauge, ShieldCheck, Sparkles, Workflow } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useEffect } from 'react';

import { GithubIcon } from '@/components/GithubIcon';
import { Nav } from '@/components/Nav';
import { Reveal } from '@/components/Reveal';
import { buttonVariants } from '@/components/ui/button-variants';
import { links } from '@/lib/sidebar';
import { cn } from '@/lib/utils';

interface Feature {
  icon: LucideIcon;
  title: string;
  body: string;
  slug: string;
}

const features: Feature[] = [
  {
    icon: FolderTree,
    title: 'Collections & requests',
    body: 'HTTP, GraphQL, gRPC and WebSocket requests in one file-based, git-diffable tree.',
    slug: 'collections-and-requests',
  },
  {
    icon: Workflow,
    title: 'Flow',
    body: 'Wire saved requests together on a canvas and run them as one workflow.',
    slug: 'flow',
  },
  {
    icon: Sparkles,
    title: 'AI Assistant',
    body: 'An agent that reads your workspace and proposes changes — nothing is written until you approve it.',
    slug: 'ai-assistant',
  },
  {
    icon: KeyRound,
    title: 'Secrets & vault',
    body: 'RocketVault, an environment variable, the OS keychain, or an external manager — never your repo.',
    slug: 'secrets-and-vault',
  },
  {
    icon: Gauge,
    title: 'Load testing',
    body: 'Phase-based or fixed-concurrency runs with a live dashboard and exportable reports.',
    slug: 'load-testing',
  },
  {
    icon: ShieldCheck,
    title: 'Contract Lock',
    body: 'Attach an SLA/API contract to a collection and track drift as the API changes.',
    slug: 'contract-lock',
  },
];

export function Home() {
  useEffect(() => {
    document.title = 'Rocket Manual';
  }, []);

  return (
    <div className="flex min-h-screen flex-col">
      <Nav />

      <main className="flex-1">
        <section className="border-b border-border">
          <div className="mx-auto w-full max-w-5xl px-5 py-20 text-center sm:px-8 sm:py-28">
            <Reveal>
              <img src="/rocket.png" alt="" aria-hidden="true" className="mx-auto size-16" />
              <h1 className="mt-6 font-heading text-4xl font-semibold tracking-tight sm:text-5xl">
                Rocket <span className="text-muted-foreground">User Manual</span>
              </h1>
              <p className="mx-auto mt-5 max-w-2xl text-base leading-relaxed text-muted-foreground sm:text-lg">
                Everything Rocket does, and how to use it — collections, Flow, the AI Assistant,
                secrets, and every feature in between.
              </p>
            </Reveal>

            <Reveal delay={80}>
              <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
                <Link to="/guide/getting-started" className={cn(buttonVariants({ variant: 'primary', size: 'lg' }))}>
                  Get started
                </Link>
                <a
                  href={links.github}
                  target="_blank"
                  rel="noreferrer"
                  className={cn(buttonVariants({ variant: 'outline', size: 'lg' }))}
                >
                  <GithubIcon />
                  View on GitHub
                </a>
              </div>
            </Reveal>
          </div>
        </section>

        <section className="mx-auto w-full max-w-6xl px-5 py-16 sm:px-8">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {features.map((feature, index) => (
              <Reveal key={feature.slug} delay={(index % 3) * 70}>
                <Link
                  to={`/guide/${feature.slug}`}
                  className="group flex h-full flex-col rounded-xl border border-border bg-card p-6 transition-[border-color,box-shadow,transform] duration-200 ease-out hover:-translate-y-0.5 hover:border-primary/40 hover:shadow-md"
                >
                  <span className="flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary transition-colors duration-200 group-hover:bg-primary/15">
                    <feature.icon className="size-5" aria-hidden="true" />
                  </span>
                  <h3 className="mt-5 font-heading text-base font-semibold tracking-tight text-card-foreground">
                    {feature.title}
                  </h3>
                  <p className="mt-2.5 flex-1 text-sm leading-relaxed text-muted-foreground">{feature.body}</p>
                </Link>
              </Reveal>
            ))}
          </div>
        </section>
      </main>

      <footer className="border-t border-border py-8 text-center">
        <p className="font-heading text-[11px] tracking-wide text-muted-foreground">
          MIT licensed. No cloud, no account required. © Numericlabs.io
        </p>
      </footer>
    </div>
  );
}
