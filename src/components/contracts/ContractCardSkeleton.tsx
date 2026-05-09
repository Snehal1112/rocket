import { Skeleton } from '@/components/ui/skeleton';

export function ContractCardSkeleton() {
  return (
    <div className='border border-border rounded-[var(--radius)] p-[18px_20px] grid grid-cols-[1fr_220px] gap-6 mb-[10px] animate-pulse'>
      <div className='space-y-3'>
        <div className='flex items-center gap-2'>
          <Skeleton className='h-3.5 w-3.5 rounded' />
          <Skeleton className='h-4 w-48' />
          <Skeleton className='h-4 w-14 rounded' />
          <Skeleton className='ml-auto h-5 w-16 rounded-md' />
        </div>
        <div className='flex gap-2'>
          <Skeleton className='h-7 w-32 rounded-full' />
          <Skeleton className='h-4 w-4 rounded' />
          <Skeleton className='h-7 w-32 rounded-full' />
        </div>
        <div className='flex gap-5'>
          <Skeleton className='h-3 w-24' />
          <Skeleton className='h-3 w-20' />
        </div>
        <div className='flex gap-1.5'>
          <Skeleton className='h-5 w-32 rounded-[4px]' />
          <Skeleton className='h-5 w-20 rounded-[4px]' />
          <Skeleton className='h-5 w-24 rounded-[4px]' />
        </div>
      </div>
      <Skeleton className='h-full rounded-[calc(var(--radius)-2px)]' />
    </div>
  );
}
