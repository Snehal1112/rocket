import { useEffect, useRef } from 'react';
import { Link } from 'react-router-dom';
import Markdown from 'react-markdown';
import rehypeSlug from 'rehype-slug';
import remarkGfm from 'remark-gfm';

import { ImageLightbox } from '@/components/ImageLightbox';
import { cn } from '@/lib/utils';

interface MarkdownPageProps {
  source: string;
  className?: string;
  /** Called once per render with the h2/h3 headings found, for the page outline. */
  onHeadings?: (headings: { id: string; text: string; level: 2 | 3 }[]) => void;
}

export function MarkdownPage({ source, className, onHeadings }: MarkdownPageProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!onHeadings || !ref.current) return;
    const nodes = ref.current.querySelectorAll('h2[id], h3[id]');
    const headings = Array.from(nodes).map((node) => ({
      id: node.id,
      text: node.textContent ?? '',
      level: (node.tagName === 'H2' ? 2 : 3) as 2 | 3,
    }));
    onHeadings(headings);
  }, [source, onHeadings]);

  return (
    <div ref={ref} className={cn('doc-content', className)}>
      <Markdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeSlug]}
        components={{
          img: ({ src, alt }) => (
            <ImageLightbox src={typeof src === 'string' ? src : ''} alt={alt ?? ''} className="doc-img my-5" />
          ),
          a: ({ href, children }) => {
            if (href?.startsWith('/')) {
              return <Link to={href}>{children}</Link>;
            }
            const external = href?.startsWith('http');
            return (
              <a href={href} target={external ? '_blank' : undefined} rel={external ? 'noreferrer' : undefined}>
                {children}
              </a>
            );
          },
        }}
      >
        {source}
      </Markdown>
    </div>
  );
}
