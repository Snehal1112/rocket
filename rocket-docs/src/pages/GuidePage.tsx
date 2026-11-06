import { useCallback, useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';

import { DocsLayout } from '@/components/DocsLayout';
import { MarkdownPage } from '@/components/MarkdownPage';
import { PageOutline } from '@/components/PageOutline';
import { content } from '@/lib/content';
import { sidebarItems } from '@/lib/sidebar';

interface Heading {
  id: string;
  text: string;
  level: 2 | 3;
}

export function GuidePage() {
  const { slug = '' } = useParams();
  const [headings, setHeadings] = useState<Heading[]>([]);
  const source = content[slug];
  const item = sidebarItems.find((entry) => entry.slug === slug);

  const onHeadings = useCallback((found: Heading[]) => setHeadings(found), []);

  useEffect(() => {
    document.title = item ? `${item.title} — Rocket Manual` : 'Page not found — Rocket Manual';
  }, [item]);

  useEffect(() => {
    window.scrollTo(0, 0);
  }, [slug]);

  if (!source) {
    return (
      <DocsLayout>
        <h1 className="text-2xl font-semibold">Page not found</h1>
        <p className="mt-3 text-muted-foreground">
          There's no guide page at <code className="rounded bg-muted px-1.5 py-0.5">{slug}</code>.
        </p>
      </DocsLayout>
    );
  }

  return (
    <DocsLayout outline={<PageOutline headings={headings} />}>
      <MarkdownPage source={source} onHeadings={onHeadings} />
    </DocsLayout>
  );
}
