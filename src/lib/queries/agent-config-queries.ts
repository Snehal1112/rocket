import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  type AgentConfig,
  deleteAgentConfig,
  listAgentConfigs,
  saveAgentConfig,
  testAgentConfig,
} from '@/lib/tauri-api';

export const agentConfigKeys = {
  list: ['agentConfigs'] as const,
};

export function useAgentConfigs() {
  return useQuery({
    queryKey: agentConfigKeys.list,
    queryFn: listAgentConfigs,
  });
}

export function useSaveAgentConfig() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (config: AgentConfig) => saveAgentConfig(config),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: agentConfigKeys.list });
    },
  });
}

export function useDeleteAgentConfig() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteAgentConfig(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: agentConfigKeys.list });
    },
  });
}

export function useTestAgentConfig() {
  return useMutation({
    mutationFn: (id: string) => testAgentConfig(id),
  });
}
