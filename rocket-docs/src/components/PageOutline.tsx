interface Heading {
  id: string;
  text: string;
  level: 2 | 3;
}

export function PageOutline({ headings }: { headings: Heading[] }) {
  if (headings.length === 0) return null;

  return (
    <nav aria-label="On this page" className="sticky top-20 hidden w-56 shrink-0 self-start xl:block">
      <p className="font-heading text-[11px] tracking-[0.14em] text-muted-foreground uppercase">On this page</p>
      <ul className="mt-3 space-y-2 border-l border-border pl-3">
        {headings.map((heading) => (
          <li key={heading.id} className={heading.level === 3 ? 'pl-3' : undefined}>
            <a
              href={`#${heading.id}`}
              className="block text-sm text-muted-foreground transition-colors hover:text-foreground"
            >
              {heading.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
