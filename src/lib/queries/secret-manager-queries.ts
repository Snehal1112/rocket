import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  deleteSecretManagerConnection,
  listSecretManagerConnections,
  type SecretManagerConnection,
  saveSecretManagerConnection,
  testSecretManagerConnection,
} from '@/lib/tauri-api';

export const secretManagerKeys = {
  list: ['secretManagerConnections'] as const,
};

export function useSecretManagerConnections() {
  return useQuery({
    queryKey: secretManagerKeys.list,
    queryFn: listSecretManagerConnections,
  });
}

export function useSaveSecretManagerConnection() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      connection,
      clientSecret,
    }: {
      connection: SecretManagerConnection;
      clientSecret?: string;
    }) => saveSecretManagerConnection(connection, clientSecret),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: secretManagerKeys.list });
    },
  });
}

export function useDeleteSecretManagerConnection() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteSecretManagerConnection(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: secretManagerKeys.list });
    },
  });
}

export function useTestSecretManagerConnection() {
  return useMutation({
    mutationFn: ({ id, vaultName }: { id: string; vaultName: string }) =>
      testSecretManagerConnection(id, vaultName),
  });
}
