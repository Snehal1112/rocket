import { type ReactNode, useEffect, useState } from 'react';
import { cn } from '@/lib/utils';

function Skeleton({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      aria-hidden='true'
      className={cn('motion-safe:animate-pulse rounded-md bg-muted', className)}
      {...props}
    />
  );
}

interface SkeletonRegionProps {
  /** What is loading, for screen readers. Say the thing, such as "Loading collections". */
  label: string;
  /** Wait this long before showing the shapes, so a fast load never flashes. */
  delayMs?: number;
  className?: string;
  children: ReactNode;
}

/** Wraps a loading placeholder: a status label for screen readers, shapes after a short delay. */
function SkeletonRegion({ label, delayMs = 150, className, children }: SkeletonRegionProps) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const timer = setTimeout(() => setVisible(true), delayMs);
    return () => clearTimeout(timer);
  }, [delayMs]);
  return (
    <div role='status' aria-busy='true' data-slot='skeleton-region' className={className}>
      <span className='sr-only'>{label}</span>
      {visible && children}
    </div>
  );
}

export { Skeleton, SkeletonRegion };
