import {
  Crosshair,
  FileText,
  Folder,
  Globe,
  Inbox,
  Layers,
  type LucideIcon,
  X,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import type { ReferenceKind } from '@/lib/assistant/types';
import type { ComposerChip } from './chips';

const KIND_ICON: Record<ReferenceKind, LucideIcon> = {
  request: FileText,
  folder: Folder,
  collection: Layers,
  environment: Globe,
  'last-response': Inbox,
};

interface ComposerChipsProps {
  chips: ComposerChip[];
  onRemove(key: string): void;
}

/** The context chips above the prompt. The focus chip shows a crosshair. */
export function ComposerChips({ chips, onRemove }: ComposerChipsProps) {
  if (chips.length === 0) return null;
  return (
    <div className='flex flex-wrap gap-1 px-2 pt-2'>
      {chips.map((chip) => {
        const Icon = chip.focus ? Crosshair : KIND_ICON[chip.item.kind];
        return (
          <Badge key={chip.key} variant='secondary' className='gap-1 pr-0.5 font-normal'>
            <Icon className='size-3 shrink-0' />
            <span className='max-w-40 truncate'>{chip.item.label}</span>
            <Button
              variant='ghost'
              size='icon'
              className='size-4 p-0'
              aria-label={`Remove ${chip.item.label}`}
              onClick={() => onRemove(chip.key)}
            >
              <X className='size-3' />
            </Button>
          </Badge>
        );
      })}
    </div>
  );
}
