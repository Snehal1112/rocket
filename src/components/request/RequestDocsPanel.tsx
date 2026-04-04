import { useState, useEffect, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { FileText } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface RequestDocsPanelProps {
  docs: string | null;
  mode: 'edit' | 'preview';
  hasSource: boolean;
  onSave: (docs: string | null) => void;
  onSwitchToEdit: () => void;
}

export function RequestDocsPanel({ docs, mode, hasSource, onSave, onSwitchToEdit }: RequestDocsPanelProps) {
  const [text, setText] = useState(docs ?? '');

  // Re-sync when docs prop changes (e.g. tab switch or external reload).
  useEffect(() => {
    setText(docs ?? '');
  }, [docs]);

  const handleSave = useCallback(() => {
    if (!hasSource) return;
    onSave(text.trim() || null);
  }, [hasSource, onSave, text]);

  if (mode === 'edit') {
    return (
      <div className="flex flex-col h-full overflow-hidden">
        <textarea
          className="flex-1 w-full bg-transparent border-none resize-none px-4 py-3.5 text-xs font-mono text-muted-foreground placeholder:text-muted-foreground/40 focus-visible:outline-none leading-relaxed"
          placeholder={'Add docs for this request...\n\nSupports **Markdown**'}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={handleSave}
          disabled={!hasSource}
        />
        <div className="flex justify-end items-center gap-2 px-3 py-2 border-t border-border shrink-0">
          {!hasSource ? (
            <span className="text-[10px] text-muted-foreground/50">
              Save this request to a collection before adding docs.
            </span>
          ) : (
            <span className="text-[10px] text-muted-foreground/50">
              Markdown supported · saves on blur
            </span>
          )}
          <Button
            size="sm"
            className="h-6 text-[10px] px-3"
            disabled={!hasSource}
            onClick={handleSave}
          >
            Save
          </Button>
        </div>
      </div>
    );
  }

  // Preview mode.
  return (
    <div className="flex-1 overflow-y-auto px-4 py-3.5 h-full">
      {text.trim() ? (
        <div className="prose-doc text-xs leading-relaxed">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
        </div>
      ) : (
        <div className="h-full flex flex-col items-center justify-center gap-3 text-center py-8">
          <FileText className="h-9 w-9 text-muted-foreground/20" />
          <div className="space-y-1">
            <p className="text-xs font-medium text-muted-foreground/60">
              No documentation yet.
            </p>
            <p className="text-[11px] text-muted-foreground/40">
              Describe what this request does, expected inputs, and example responses.
            </p>
          </div>
          {hasSource && (
            <Button variant="outline" size="sm" className="text-xs h-7 mt-1" onClick={onSwitchToEdit}>
              + Add Documentation
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
