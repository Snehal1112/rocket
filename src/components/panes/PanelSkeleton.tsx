import { Skeleton, SkeletonRegion } from '@/components/ui/skeleton';

/** A tab that is not a code editor while it loads: a title row, a toolbar and a few content rows. */
export function PanelSkeleton({ label = 'Loading' }: { label?: string }) {
  return (
    <SkeletonRegion label={label} className='flex h-full flex-col'>
      <div className='flex items-center gap-3 border-b border-border px-4 py-3'>
        <Skeleton className='size-4 shrink-0' />
        <Skeleton className='h-4 w-40' />
        <Skeleton className='ml-auto h-7 w-20' />
      </div>
      <div className='flex flex-col gap-3 p-4'>
        <Skeleton className='h-8 w-full' />
        <Skeleton className='h-8 w-full' />
        <Skeleton className='h-8 w-5/6' />
        <Skeleton className='h-8 w-2/3' />
      </div>
    </SkeletonRegion>
  );
}
