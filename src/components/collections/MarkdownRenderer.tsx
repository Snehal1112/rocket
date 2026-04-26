import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import oneDark from 'react-syntax-highlighter/dist/esm/styles/prism/one-dark';
import oneLight from 'react-syntax-highlighter/dist/esm/styles/prism/one-light';
import remarkGfm from 'remark-gfm';
import { cn } from '@/lib/utils';

interface MarkdownRendererProps {
  children: string;
  className?: string;
}

function useIsDark() {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains('dark'),
  );
  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
    return () => observer.disconnect();
  }, []);
  return isDark;
}

export function MarkdownRenderer({ children, className }: MarkdownRendererProps) {
  const isDark = useIsDark();

  return (
    <div className={cn('prose-doc', className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          code({ className: cls, children: ch, ...rest }) {
            // language-* className signals a fenced code block in react-markdown v10.
            const match = /language-(\w+)/.exec(cls ?? '');
            const code = String(ch).replace(/\n$/, '');

            if (match) {
              return (
                <SyntaxHighlighter
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  style={(isDark ? oneDark : oneLight) as any}
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
