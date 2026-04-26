import { Check, FileText, Loader2, Save } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { cn } from '@/lib/utils';
import { MarkdownRenderer } from './MarkdownRenderer';

interface MarkdownEditorProps {
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  /** Controlled mode — when omitted the component manages mode internally */
  mode?: 'edit' | 'preview';
  onModeChange?: (mode: 'edit' | 'preview') => void;
  /** When provided, a Save button is shown in the edit footer */
  onSave?: () => void;
  saveState?: SaveButtonState;
  isDirty?: boolean;
}

export function MarkdownEditor({
  value,
  onChange,
  onBlur,
  mode: controlledMode,
  onModeChange,
  onSave,
  saveState = 'idle',
  isDirty = false,
}: MarkdownEditorProps) {
  const [internalMode, setInternalMode] = useState<'edit' | 'preview'>('preview');
  const mode = controlledMode ?? internalMode;

  function handleModeChange(next: 'edit' | 'preview') {
    if (onModeChange) {
      onModeChange(next);
    } else {
      setInternalMode(next);
    }
  }

  return (
    <Card className='flex-1 flex flex-col overflow-hidden'>
      <CardHeader className='flex flex-row items-center justify-between py-2.5 px-4 shrink-0'>
        <div className='flex items-center gap-2'>
          <FileText className='h-3.5 w-3.5 text-muted-foreground' />
          <span className='text-xs font-semibold text-muted-foreground'>Documentation</span>
        </div>
        <Tabs value={mode} onValueChange={(v) => handleModeChange(v as 'edit' | 'preview')}>
          <TabsList className='h-6'>
            <TabsTrigger value='edit' className='text-[10px] px-2.5 py-0.5'>
              Edit
            </TabsTrigger>
            <TabsTrigger value='preview' className='text-[10px] px-2.5 py-0.5'>
              Preview
            </TabsTrigger>
          </TabsList>
        </Tabs>
      </CardHeader>

      <CardContent className='flex-1 p-0 overflow-hidden flex flex-col'>
        {mode === 'edit' && (
          <div className='flex-1 flex flex-col overflow-hidden'>
            <Textarea
              className='flex-1 w-full bg-transparent border-none resize-none px-4 py-3.5 text-xs font-mono text-muted-foreground placeholder:text-muted-foreground/40 focus-visible:outline-none focus-visible:ring-0 leading-relaxed'
              placeholder={'Add documentation...\n\nSupports **Markdown**'}
              value={value}
              onChange={(e) => onChange(e.target.value)}
              onBlur={onBlur}
            />
            {onSave && (
              <div className='flex justify-end items-center gap-2 px-3 py-2 border-t border-border shrink-0'>
                <span className='text-[10px] text-muted-foreground/50'>
                  Markdown supported · saves on blur
                </span>
                <Button
                  size='sm'
                  className={cn(
                    'h-6 text-[10px] px-3 gap-1',
                    saveState === 'success' && 'text-green-600',
                  )}
                  onClick={onSave}
                  disabled={!isDirty || saveState !== 'idle'}
                >
                  {saveState === 'saving' ? (
                    <Loader2 className='h-3 w-3 animate-spin' />
                  ) : saveState === 'success' ? (
                    <Check className='h-3 w-3' />
                  ) : (
                    <Save className='h-3 w-3' />
                  )}
                  {saveState === 'success' ? 'Saved' : 'Save'}
                </Button>
              </div>
            )}
          </div>
        )}

        {mode === 'preview' && (
          <div className='flex-1 overflow-y-auto px-4 py-3.5'>
            {value.trim() ? (
              <MarkdownRenderer>{value}</MarkdownRenderer>
            ) : (
              <div className='h-full flex flex-col items-center justify-center gap-3 text-center py-8'>
                <FileText className='w-9 h-9 text-muted-foreground/50' />
                <div className='space-y-1.5'>
                  <p className='text-sm font-medium text-foreground'>No documentation yet</p>
                  <p className='text-xs font-medium text-muted-foreground leading-relaxed'>
                    Add an overview, setup instructions, or key workflows to help your team.
                  </p>
                </div>
                <Button
                  variant='outline'
                  size='sm'
                  className='text-xs h-7'
                  onClick={() => handleModeChange('edit')}
                >
                  <FileText className='h-3 w-3 mr-1.5' />
                  Add Documentation
                </Button>
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
