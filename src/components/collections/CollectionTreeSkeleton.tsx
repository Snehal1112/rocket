import { Skeleton, SkeletonRegion } from '@/components/ui/skeleton';
import { TREE_CHILDREN, TREE_CONTENT } from '@/components/ui/tree';
import { cn } from '@/lib/utils';

// Label widths vary so the rows read as names, not a table. They are fixed, never random.
function Row({ chevron, icon, label }: { chevron?: boolean; icon?: boolean; label: string }) {
  return (
    <div className={cn(TREE_CONTENT, 'cursor-default')}>
      <span className='flex size-4 shrink-0 items-center justify-center'>
        {chevron && <Skeleton className='size-3 rounded-sm' />}
      </span>
      {icon && <Skeleton className='size-4 shrink-0 rounded-sm' />}
      <Skeleton className={cn('h-3', label)} />
    </div>
  );
}

function RequestRow({ label }: { label: string }) {
  return (
    <div className={cn(TREE_CONTENT, 'cursor-default')}>
      <span className='size-4 shrink-0' />
      <Skeleton className='h-2.5 w-8 shrink-0' />
      <Skeleton className={cn('h-3', label)} />
    </div>
  );
}

/** The collection tree while its collections load: the same rows, indent and guide as the tree. */
export function CollectionTreeSkeleton() {
  return (
    <SkeletonRegion label='Loading collections' className='px-1 pb-2'>
      <Row chevron icon label='w-32' />
      <div className={TREE_CHILDREN}>
        <Row chevron icon label='w-24' />
        <Row chevron icon label='w-28' />
        <RequestRow label='w-32' />
        <RequestRow label='w-20' />
      </div>
      <Row chevron icon label='w-28' />
      <Row chevron icon label='w-36' />
    </SkeletonRegion>
  );
}
