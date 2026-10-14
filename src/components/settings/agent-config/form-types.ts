// Which ACP agent a form edits. Matches AgentKind, where a missing value means claude.
export type AgentKindValue = 'claude' | 'gemini' | 'generic';

// The credential slice of the agent form. Persisted values are unchanged.
export interface CredentialFormState {
  credentialSource: 'vault' | 'env' | 'keychain';
  // UI label: "Agent reads it as". The variable the agent receives.
  credentialEnvVar: string;
  // UI label: "Rocket reads it from". The variable in Rocket's own environment.
  sourceEnvVar: string;
  localCredential: string;
  removeSavedCredential: boolean;
  vaultConnectionId: string;
  // For Azure this is saved as the connection label.
  vaultName: string;
  vaultSecretId: string;
  vaultSecretName: string;
}

export type CredentialErrors = Partial<Record<keyof CredentialFormState, string | null>>;

// The credential state of a brand new agent.
export const EMPTY_CREDENTIAL_FORM: CredentialFormState = {
  credentialSource: 'vault',
  credentialEnvVar: '',
  sourceEnvVar: '',
  localCredential: '',
  removeSavedCredential: false,
  vaultConnectionId: '',
  vaultName: '',
  vaultSecretId: '',
  vaultSecretName: '',
};
