import { useEffect, useState } from 'react';
import { type InheritedAuthSource, resolveInheritedAuthSource } from '@/lib/inherited-auth';

/**
 * Where a request's inherited authorization comes from. Resolves only while `enabled`, so it
 * costs nothing unless the Auth tab is open on a request set to Inherit.
 */
export function useInheritedAuthSource(
  collection: string | undefined,
  requestPath: string | undefined,
  enabled: boolean,
): InheritedAuthSource | undefined {
  const [source, setSource] = useState<InheritedAuthSource>();

  useEffect(() => {
    if (!enabled || !collection || !requestPath) {
      setSource(undefined);
      return;
    }
    let cancelled = false;
    void resolveInheritedAuthSource(collection, requestPath).then((s) => {
      if (!cancelled) setSource(s);
    });
    return () => {
      cancelled = true;
    };
  }, [collection, requestPath, enabled]);

  return source;
}
