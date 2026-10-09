import { useEffect, useState } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import oneDark from 'react-syntax-highlighter/dist/esm/styles/prism/one-dark';
import oneLight from 'react-syntax-highlighter/dist/esm/styles/prism/one-light';
import remarkGfm from 'remark-gfm';
import { cn } from '@/lib/utils';

interface MarkdownRendererProps {
  children: string;
  className?: string;
  // When given, rendered over each fenced (language-tagged) code block —
  // e.g. an "Insert" button in the AI Assist chat panel.
  renderCodeActions?: (code: string, language?: string) => React.ReactNode;
  // For text written by an agent. Images are never rendered, so nothing is
  // fetched, and links are plain text that shows their URL, so nothing opens.
  restricted?: boolean;
}

// Only these schemes are shown as a URL. javascript:, data:, file: and the
// rest are blocked outright.
const SHOWN_URL = /^(https?:|mailto:)/i;

const RESTRICTED_COMPONENTS: Components = {
  img({ alt }) {
    return (
      <span className='text-muted-foreground'>
        {alt ? `[image omitted: ${alt}]` : '[image omitted]'}
      </span>
    );
  },
  a({ href, children: ch }) {
    const url = href?.trim() ?? '';
    return (
      <span>
        {ch}{' '}
        {SHOWN_URL.test(url) ? (
          <span className='break-all text-muted-foreground'>({url})</span>
        ) : (
          <span className='text-muted-foreground'>[link blocked]</span>
        )}
      </span>
    );
  },
};

function useIsDark() {
  const [isDark, setIsDark] = useState(() => document.documentElement.classList.contains('dark'));
  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
    return () => observer.disconnect();
  }, []);
  return isDark;
}

export function MarkdownRenderer({
  children,
  className,
  renderCodeActions,
  restricted = false,
}: MarkdownRendererProps) {
  const isDark = useIsDark();

  return (
    <div className={cn('prose-doc', className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          ...(restricted ? RESTRICTED_COMPONENTS : {}),
          code({ className: cls, children: ch, ...rest }) {
            // language-* className signals a fenced code block in react-markdown v10.
            const match = /language-(\w+)/.exec(cls ?? '');
            const code = String(ch).replace(/\n$/, '');

            if (match) {
              return (
                <div className='relative'>
                  <SyntaxHighlighter
                    style={(isDark ? oneDark : oneLight) as Record<string, React.CSSProperties>}
                    language={match[1]}
                    PreTag='div'
                    customStyle={{
                      margin: '0 0 1rem',
                      borderRadius: '8px',
                      fontSize: '0.8125rem',
                      ...(isDark ? {} : { background: 'hsl(var(--muted))' }),
                    }}
                    codeTagProps={{ style: { fontFamily: 'var(--font-mono)' } }}
                  >
                    {code}
                  </SyntaxHighlighter>
                  {renderCodeActions && (
                    <div className='absolute right-2 top-2'>
                      {renderCodeActions(code, match[1])}
                    </div>
                  )}
                </div>
              );
            }

            return (
              <code className={cn(cls)} {...rest}>
                {ch}
              </code>
            );
          },
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}
