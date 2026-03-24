import { useState } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ResponseHeadersTable } from './ResponseHeadersTable';
import type { ResponseState } from '@/types/pane-types';

type ViewTab = ResponseState['activeView'];

interface ResponseBodyViewerProps {
  response: ResponseState;
}

// ---- JSON syntax highlighting ------------------------------------------------

// Token types produced by the JSON tokenizer.
type TokenType = 'key' | 'string' | 'number' | 'boolean' | 'null' | 'punctuation';

interface Token {
  type: TokenType;
  value: string;
}

// Color map for each token type.
const TOKEN_COLORS: Record<TokenType, string> = {
  key: 'text-purple-500',
  string: 'text-emerald-500',
  number: 'text-amber-500',
  boolean: 'text-blue-500',
  null: 'text-gray-500',
  punctuation: 'text-muted-foreground',
};

// Regex that matches the next meaningful JSON token in a string.
const TOKEN_RE =
  /("(?:[^"\\]|\\.)*")\s*(:)|("(?:[^"\\]|\\.)*")|(true|false)|(null)|(-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)|([{}\[\],:])/g;

function tokenizeLine(line: string): Token[] {
  const tokens: Token[] = [];
  let lastIndex = 0;
  TOKEN_RE.lastIndex = 0;

  let match: RegExpExecArray | null;
  while ((match = TOKEN_RE.exec(line)) !== null) {
    // Emit any plain text that precedes this token as punctuation.
    if (match.index > lastIndex) {
      tokens.push({ type: 'punctuation', value: line.slice(lastIndex, match.index) });
    }

    if (match[1] !== undefined) {
      // Object key (string followed by colon).
      tokens.push({ type: 'key', value: match[1] });
      // Emit the colon separately.
      tokens.push({ type: 'punctuation', value: ':' });
      TOKEN_RE.lastIndex = match.index + match[1].length + match[0].length - match[1].length - 1;
    } else if (match[3] !== undefined) {
      tokens.push({ type: 'string', value: match[3] });
    } else if (match[4] !== undefined) {
      tokens.push({ type: 'boolean', value: match[4] });
    } else if (match[5] !== undefined) {
      tokens.push({ type: 'null', value: match[5] });
    } else if (match[6] !== undefined) {
      tokens.push({ type: 'number', value: match[6] });
    } else if (match[7] !== undefined) {
      tokens.push({ type: 'punctuation', value: match[7] });
    }

    lastIndex = TOKEN_RE.lastIndex;
  }

  // Trailing plain text.
  if (lastIndex < line.length) {
    tokens.push({ type: 'punctuation', value: line.slice(lastIndex) });
  }

  return tokens;
}

// Renders a pretty-printed JSON string with per-token color spans.
function highlightJson(json: string): React.ReactNode[] {
  return json.split('\n').map((line, lineIdx) => {
    const tokens = tokenizeLine(line);
    return (
      <div key={lineIdx}>
        {tokens.map((tok, i) => (
          <span key={i} className={TOKEN_COLORS[tok.type]}>
            {tok.value}
          </span>
        ))}
      </div>
    );
  });
}

// Try to detect and pretty-print JSON; fall back to raw body.
function formatBody(body: string): { formatted: string; isJson: boolean } {
  try {
    const parsed = JSON.parse(body);
    return { formatted: JSON.stringify(parsed, null, 2), isJson: true };
  } catch {
    return { formatted: body, isJson: false };
  }
}

// Attempt basic XML indentation by re-serialising through DOMParser.
function formatXml(xml: string): string {
  try {
    const parser = new DOMParser();
    const doc = parser.parseFromString(xml, 'application/xml');
    const error = doc.querySelector('parsererror');
    if (error) return xml;
    // Serialise with indentation via XMLSerializer + a simple indent pass.
    const serialised = new XMLSerializer().serializeToString(doc);
    // Basic indent: add newlines around tags.
    return serialised
      .replace(/></g, '>\n<')
      .split('\n')
      .map((line) => line.trim())
      .join('\n');
  } catch {
    return xml;
  }
}

// ---- Main component ----------------------------------------------------------

export function ResponseBodyViewer({ response }: ResponseBodyViewerProps) {
  // Fall back to 'pretty' if activeView is not set.
  const [activeView, setActiveView] = useState<ViewTab>(
    response.activeView ?? 'pretty',
  );

  const { formatted, isJson } = formatBody(response.body);

  // Detect XML from Content-Type header.
  const contentType = response.headers
    .find((h) => h.key.toLowerCase() === 'content-type')
    ?.value.toLowerCase() ?? '';
  const isXml = contentType.includes('xml') || (!isJson && response.body.trimStart().startsWith('<'));

  const prettyBody = isJson
    ? formatted
    : isXml
    ? formatXml(response.body)
    : response.body;

  return (
    <div className="flex h-full flex-col">
      {/* View tab switcher using shadcn Tabs. */}
      <Tabs value={activeView} onValueChange={(val) => setActiveView(val as ViewTab)}>
        <div className="border-b border-border px-1">
          <TabsList className="h-8 bg-transparent p-0">
            <TabsTrigger value="pretty" className="h-7 rounded-none bg-transparent text-xs data-[state=active]:shadow-none">
              Pretty
            </TabsTrigger>
            <TabsTrigger value="raw" className="h-7 rounded-none bg-transparent text-xs data-[state=active]:shadow-none">
              Raw
            </TabsTrigger>
            <TabsTrigger value="preview" className="h-7 rounded-none bg-transparent text-xs data-[state=active]:shadow-none">
              Preview
            </TabsTrigger>
            <TabsTrigger value="headers" className="h-7 rounded-none bg-transparent text-xs data-[state=active]:shadow-none">
              Headers
            </TabsTrigger>
          </TabsList>
        </div>

        {/* View content. */}
        <div className="flex-1 overflow-auto p-3">
          <TabsContent value="pretty" className="mt-0">
            <pre className="whitespace-pre-wrap break-all font-mono text-xs leading-5">
              {isJson
                ? highlightJson(prettyBody)
                : prettyBody}
            </pre>
          </TabsContent>

          <TabsContent value="raw" className="mt-0">
            <pre className="whitespace-pre-wrap break-all font-mono text-xs leading-5 text-foreground">
              {response.body}
            </pre>
          </TabsContent>

          <TabsContent value="preview" className="mt-0">
            <iframe
              srcDoc={response.body}
              sandbox=""
              className="h-full min-h-48 w-full border-0 bg-white"
              title="Response preview"
            />
          </TabsContent>

          <TabsContent value="headers" className="mt-0">
            <ResponseHeadersTable headers={response.headers} />
          </TabsContent>
        </div>
      </Tabs>
    </div>
  );
}
