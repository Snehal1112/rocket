import { Check, FileText, PenLine } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { Button } from '@/components/ui/button';
import { Card } from '../ui/card';

interface RequestDocsPanelProps {
  docs: string | null;
  mode: 'edit' | 'preview';
  hasSource: boolean;
  onSave: (docs: string | null) => void;
  onSwitchToEdit: () => void;
}

export function RequestDocsPanel({
  docs,
  mode,
  hasSource,
  onSave,
  onSwitchToEdit,
}: RequestDocsPanelProps) {
  const [text, setText] = useState(docs ?? '');
  const [justSaved, setJustSaved] = useState(false);

  // Re-sync when docs prop changes (e.g. tab switch or external reload).
  useEffect(() => {
    setText(docs ?? '');
  }, [docs]);

  const handleSave = useCallback(() => {
    if (!hasSource) return;
    onSave(text.trim() || null);
    setJustSaved(true);
    setTimeout(() => setJustSaved(false), 2000);
  }, [hasSource, onSave, text]);

  if (mode === 'edit') {
    return (
      // Negative margin cancels the parent p-3 so the editor fills edge-to-edge.
      <Card className='flex flex-col overflow-hidden h-full px-3 py-3'>
        {!hasSource && (
          <div className='border-b border-amber-500/20 bg-amber-500/5 px-3 py-2 flex items-center gap-2'>
            <span className='text-[11px] text-amber-600 dark:text-amber-400'>
              Save this request to a collection before adding docs.
            </span>
          </div>
        )}
        <div className='relative flex-1 overflow-hidden'>
          {/* Subtle left accent stripe. */}
          <div className='absolute left-0 top-0 bottom-0 w-[3px]' />
          <textarea
            className='h-full w-full bg-transparent border-none resize-none pl-4 pr-3 py-3 text-xs font-mono text-foreground/80 placeholder:text-muted-foreground/30 focus-visible:outline-none leading-[1.7]'
            placeholder={
              '# Request Documentation\n\nDescribe what this endpoint does...\n\nSupports **Markdown** syntax.'
            }
            value={text}
            onChange={(e) => setText(e.target.value)}
            onBlur={handleSave}
            disabled={!hasSource}
          />
        </div>
        <div className='flex justify-between items-center gap-2 px-3 py-3 border-t border-border/60 bg-muted/20 shrink-0'>
          <span className='text-[10px] text-muted-foreground/40 font-mono'>
            Markdown · auto-saves on blur
          </span>
          <Button
            size='sm'
            className='h-6 text-[10px] px-3 gap-1.5'
            disabled={!hasSource}
            onClick={handleSave}
          >
            {justSaved ? (
              <>
                <Check className='h-3 w-3' />
                Saved
              </>
            ) : (
              'Save'
            )}
          </Button>
        </div>
      </Card>
    );
  }

  // Preview mode — outer p-3 handles all spacing; no extra internal padding.
  return (
    <Card className='h-full overflow-y-auto px-3 py-3 bg-transparent'>
      {text.trim() ? (
        <MarkdownRenderer>{text}</MarkdownRenderer>
      ) : (
        <div className='h-full flex flex-col items-center justify-center gap-4 text-center py-8'>
          <div className='relative'>
            <div className='h-14 w-14 rounded-xl bg-muted/40 border border-border/50 flex items-center justify-center'>
              <FileText className='h-6 w-6 text-muted-foreground/30' />
            </div>
            {hasSource && (
              <div className='absolute -bottom-1 -right-1 h-5 w-5 rounded-full bg-background border border-border flex items-center justify-center'>
                <PenLine className='h-2.5 w-2.5 text-muted-foreground/50' />
              </div>
            )}
          </div>
          <div className='space-y-1.5'>
            <p className='text-xs font-medium text-muted-foreground/70'>No documentation yet</p>
            <p className='text-[11px] text-muted-foreground/40 max-w-[220px] leading-relaxed'>
              Document expected inputs, outputs, and usage examples for this request.
            </p>
          </div>
          {hasSource && (
            <Button
              variant='outline'
              size='sm'
              className='text-xs h-7 mt-1 gap-1.5'
              onClick={onSwitchToEdit}
            >
              <PenLine className='h-3 w-3' />
              Add Documentation
            </Button>
          )}
        </div>
      )}
    </Card>
  );
}
