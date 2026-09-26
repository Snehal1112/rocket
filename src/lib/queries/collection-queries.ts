import { useQuery } from '@tanstack/react-query';
import { listCollections } from '@/lib/tauri-api';

export const collectionKeys = {
  all: ['collections'] as const,
};

export function useCollections() {
  return useQuery({
    queryKey: collectionKeys.all,
    queryFn: listCollections,
  });
}
