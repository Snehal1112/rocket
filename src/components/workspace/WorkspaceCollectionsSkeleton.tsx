import { Card, CardContent } from '@/components/ui/card';
import { Skeleton, SkeletonRegion } from '@/components/ui/skeleton';

const NAME_WIDTHS = ['w-40', 'w-32', 'w-48'];

/** The workspace overview's collection list while it loads: three rows like the real cards. */
export function WorkspaceCollectionsSkeleton() {
  return (
    <SkeletonRegion label='Loading collections'>
      <Card className='bg-card border rounded-md'>
        <CardContent className='p-0'>
          {NAME_WIDTHS.map((width) => (
            <div
              key={width}
              className='flex items-center gap-3 px-4 py-3 border-b border-border last:border-b-0'
            >
              <Skeleton className='size-5 shrink-0' />
              <div className='flex-1 min-w-0 space-y-1.5'>
                <Skeleton className={`h-3.5 ${width}`} />
                <Skeleton className='h-3 w-64 max-w-full' />
              </div>
              <Skeleton className='h-3 w-10 shrink-0' />
            </div>
          ))}
        </CardContent>
      </Card>
    </SkeletonRegion>
  );
}
