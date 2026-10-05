import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { FileDown } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { base64ToBytes, isPreviewableImage, suggestedFileName } from '@/lib/response-binary';
import type { ResponseState } from '@/types/pane-types';

interface BinaryResponsePanelProps {
  response: ResponseState;
  sizeLabel: string;
}

// Shown instead of the text views when the response body is not text.
export function BinaryResponsePanel({ response, sizeLabel }: BinaryResponsePanelProps) {
  const contentType =
    response.headers.find((h) => h.key.toLowerCase() === 'content-type')?.value ?? '';
  const payload = response.bodyBase64;
  const type = contentType.split(';')[0].trim() || 'unknown type';

  const handleSave = async () => {
    if (!payload) return;
    try {
      const path = await save({ defaultPath: suggestedFileName(response.headers, contentType) });
      if (!path) return;
      await writeFile(path, base64ToBytes(payload));
      toast.success('Saved response body');
    } catch {
      toast.error('Could not save the response body');
    }
  };

  return (
    <div className='flex h-full flex-col gap-3 overflow-auto p-3'>
      <div className='flex items-center gap-3 text-xs text-muted-foreground'>
        <span>Binary response</span>
        <span className='font-mono'>{type}</span>
        <span>{sizeLabel}</span>
        {payload && (
          <Button variant='outline' size='sm' className='ml-auto' onClick={handleSave}>
            <FileDown className='mr-1.5 h-3.5 w-3.5' />
            Save to file
          </Button>
        )}
      </div>
      {!payload && (
        <p className='text-xs text-muted-foreground'>
          This body is too large to preview or save in the app ({sizeLabel}).
        </p>
      )}
      {payload && isPreviewableImage(contentType) && (
        <img
          src={`data:${type};base64,${payload}`}
          alt='Response preview'
          className='max-h-full max-w-full self-start rounded border border-border object-contain'
        />
      )}
    </div>
  );
}
