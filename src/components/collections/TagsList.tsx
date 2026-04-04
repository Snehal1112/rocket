import { Tag } from 'lucide-react';
import type { Collection, CollectionItem } from '@/lib/tauri-api';

interface TagCount {
  tag: string;
  count: number;
}

function collectTags(items: CollectionItem[]): Map<string, number> {
  const counts = new Map<string, number>();

  for (const item of items) {
    if (item.type === 'request' && item.tags) {
      for (const tag of item.tags) {
        counts.set(tag, (counts.get(tag) || 0) + 1);
      }
    }
    if (item.type === 'folder' && item.items) {
      const sub = collectTags(item.items);
      for (const [tag, count] of sub) {
        counts.set(tag, (counts.get(tag) || 0) + count);
      }
    }
  }

  return counts;
}

interface TagsListProps {
  collection: Collection | null;
}

export function TagsList({ collection }: TagsListProps) {
  if (!collection) return null;

  const tagMap = collectTags(collection.root.items);
  const tags: TagCount[] = Array.from(tagMap.entries())
    .map(([tag, count]) => ({ tag, count }))
    .sort((a, b) => b.count - a.count || a.tag.localeCompare(b.tag));

  if (tags.length === 0) {
    return (
      <p className='text-sm text-muted-foreground italic py-4 text-center'>
        No tags found. Add tags to requests to see them here.
      </p>
    );
  }

  return (
    <div className='flex flex-wrap gap-2 py-2'>
      {tags.map(({ tag, count }) => (
        <span
          key={tag}
          className='inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-muted text-sm'
        >
          <Tag className='h-3.5 w-3.5 text-muted-foreground' />
          {tag}
          <span className='text-xs text-muted-foreground'>({count})</span>
        </span>
      ))}
    </div>
  );
}
