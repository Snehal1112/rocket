const keyFor = (agentConfigId: string) => `rocket-api:assistant-model:${agentConfigId}`;

/** The model last chosen for this agent config, used when the next session starts. */
export function loadRememberedModel(agentConfigId: string): string | undefined {
  try {
    return localStorage.getItem(keyFor(agentConfigId)) ?? undefined;
  } catch {
    // Blocked storage means no remembered model. The agent's default is used.
    return undefined;
  }
}

export function rememberModel(agentConfigId: string, model: string): void {
  try {
    localStorage.setItem(keyFor(agentConfigId), model);
  } catch {
    // Blocked storage only loses the remembered choice.
  }
}
