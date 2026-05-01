import { QueryClient } from '@tanstack/react-query';

let client: QueryClient | null = null;

export function setQueryClient(qc: QueryClient) {
  client = qc;
}

export function getQueryClient(): QueryClient {
  if (!client) throw new Error('QueryClient not initialised');
  return client;
}
