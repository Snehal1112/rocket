import type {
  ExternalSecretBinding,
  SecretManagerConnection,
  SecretProviderKind,
} from '@/lib/tauri-api';

// The form fields a provider's connection needs. Each provider's own work adds
// its entry here, and the connection dialog renders from this list.
export type ConnectionField =
  | 'baseUrl'
  | 'clientId'
  | 'clientSecret'
  | 'verifySsl'
  | 'allowInsecureHttp';

export interface SecretProviderDescriptor {
  kind: SecretProviderKind;
  label: string;
  // False until the provider's own implementation ships.
  selectable: boolean;
  connectionFields: readonly ConnectionField[];
  // What a binding's scope field means for this provider.
  scopeLabel: string;
  scopePlaceholder: string;
  supportsCertificates: boolean;
}

export const SECRET_PROVIDERS: readonly SecretProviderDescriptor[] = [
  {
    kind: 'rocketvault',
    label: 'RocketVault',
    selectable: true,
    connectionFields: ['baseUrl', 'clientId', 'clientSecret', 'verifySsl', 'allowInsecureHttp'],
    scopeLabel: 'Vault Name',
    scopePlaceholder: 'Vault name',
    supportsCertificates: true,
  },
  {
    kind: 'azure',
    label: 'Azure Key Vault',
    selectable: false,
    connectionFields: [],
    scopeLabel: 'Vault',
    scopePlaceholder: 'Vault',
    supportsCertificates: false,
  },
  {
    kind: 'aws',
    label: 'AWS Secrets Manager',
    selectable: false,
    connectionFields: [],
    scopeLabel: 'Region',
    scopePlaceholder: 'Region',
    supportsCertificates: false,
  },
  {
    kind: 'hashicorp',
    label: 'HashiCorp Vault',
    selectable: false,
    connectionFields: [],
    scopeLabel: 'Mount',
    scopePlaceholder: 'Mount',
    supportsCertificates: false,
  },
  {
    kind: 'gcp',
    label: 'Google Secret Manager',
    selectable: false,
    connectionFields: [],
    scopeLabel: 'Project',
    scopePlaceholder: 'Project',
    supportsCertificates: false,
  },
];

const ROCKETVAULT = SECRET_PROVIDERS[0] as SecretProviderDescriptor;

// An absent provider is a RocketVault connection from before providers existed.
export function getProviderDescriptor(kind?: SecretProviderKind | null): SecretProviderDescriptor {
  return SECRET_PROVIDERS.find((p) => p.kind === kind) ?? ROCKETVAULT;
}

function connectionOf(
  binding: ExternalSecretBinding,
  connections: SecretManagerConnection[],
): SecretManagerConnection | undefined {
  return connections.find((c) => c.id === binding.connectionId);
}

// The External Secrets column header. RocketVault calls it a vault name and the
// other providers use different words, so a mixed list falls back to "Scope".
export function bindingScopeColumnLabel(
  bindings: ExternalSecretBinding[],
  connections: SecretManagerConnection[],
): string {
  const labels = new Set(
    bindings.map((b) => getProviderDescriptor(connectionOf(b, connections)?.provider).scopeLabel),
  );
  if (labels.size === 0) return ROCKETVAULT.scopeLabel;
  if (labels.size === 1) return [...labels][0] as string;
  return 'Scope';
}

// A vault certificate needs a binding whose connection can supply certificates.
// While connections are loading the answer is true, so the button does not flicker.
export function canAddVaultCertificate(
  bindings: ExternalSecretBinding[],
  connections: SecretManagerConnection[],
  connectionsLoaded: boolean,
): boolean {
  if (!connectionsLoaded) return true;
  return bindings.some((b) => {
    const connection = connectionOf(b, connections);
    return !!connection && getProviderDescriptor(connection.provider).supportsCertificates;
  });
}
