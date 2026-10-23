import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Skeleton, SkeletonRegion } from '@/components/ui/skeleton';

function CardBlock({ title, rows }: { title: string; rows: string[] }) {
  return (
    <Card>
      <CardHeader className='pb-2 pt-4 px-4'>
        <Skeleton className={title} />
      </CardHeader>
      <CardContent className='px-4 pb-4 space-y-2'>
        {rows.map((row) => (
          <Skeleton key={row} className='h-7 w-full' />
        ))}
      </CardContent>
    </Card>
  );
}

/** The collection overview while the collection loads: its header, tab bar and two cards. */
export function CollectionOverviewSkeleton() {
  return (
    <SkeletonRegion label='Loading collection' className='flex h-full flex-col overflow-hidden'>
      <div className='shrink-0 border-b border-border/70 px-6 pt-5 pb-0'>
        <div className='flex items-center gap-2.5 mb-1'>
          <Skeleton className='size-5 shrink-0' />
          <Skeleton className='h-6 w-56' />
        </div>
        <div className='pl-8 mb-4'>
          <Skeleton className='h-4 w-40' />
        </div>
        <div className='flex items-center gap-4 h-9'>
          <Skeleton className='h-3.5 w-16' />
          <Skeleton className='h-3.5 w-20' />
          <Skeleton className='h-3.5 w-14' />
        </div>
      </div>
      <div className='flex h-full overflow-hidden'>
        <div className='flex-1 min-w-0 border-r border-border'>
          <div className='p-4 flex flex-col gap-4'>
            <CardBlock title='h-4 w-28' rows={['name', 'value']} />
            <CardBlock title='h-4 w-20' rows={['first', 'second', 'third', 'fourth']} />
          </div>
        </div>
      </div>
    </SkeletonRegion>
  );
}
