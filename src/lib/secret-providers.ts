import type {
  ExternalSecretBinding,
  SecretManagerConnection,
  SecretProviderKind,
} from '@/lib/tauri-api';

// The form fields a provider's connection needs. Each provider's own work adds
// its entry here, and the connection dialog renders from this list.
export type ConnectionField =
  | 'baseUrl'
  | 'tenantId'
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
  // Overrides for the default field labels and placeholders below.
  fieldLabels?: Partial<Record<ConnectionField, string>>;
  fieldPlaceholders?: Partial<Record<ConnectionField, string>>;
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
    selectable: true,
    connectionFields: ['baseUrl', 'tenantId', 'clientId', 'clientSecret'],
    fieldLabels: { baseUrl: 'Vault URL' },
    fieldPlaceholders: { baseUrl: 'https://my-vault.vault.azure.net' },
    // The connection already names the vault, so the value is only a label.
    scopeLabel: 'Vault name',
    scopePlaceholder: 'Any name (the connection sets the vault)',
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

const DEFAULT_FIELD_LABELS: Record<ConnectionField, string> = {
  baseUrl: 'Base URL',
  tenantId: 'Tenant ID',
  clientId: 'Client ID',
  clientSecret: 'Client Secret',
  verifySsl: 'Verify SSL',
  allowInsecureHttp: 'Allow insecure HTTP',
};

export function connectionFieldLabel(
  descriptor: SecretProviderDescriptor,
  field: ConnectionField,
): string {
  return descriptor.fieldLabels?.[field] ?? DEFAULT_FIELD_LABELS[field];
}

export function connectionFieldPlaceholder(
  descriptor: SecretProviderDescriptor,
  field: ConnectionField,
): string | undefined {
  return descriptor.fieldPlaceholders?.[field];
}

// The text fields that must be filled in before a connection can be saved.
const REQUIRED_TEXT_FIELDS: readonly ConnectionField[] = ['baseUrl', 'tenantId', 'clientId'];

export function requiredFieldsMessage(descriptor: SecretProviderDescriptor): string {
  const fields = REQUIRED_TEXT_FIELDS.filter((f) => descriptor.connectionFields.includes(f)).map(
    (f) => connectionFieldLabel(descriptor, f),
  );
  if (fields.length === 0) return 'Label is required.';
  const labels = ['Label', ...fields];
  return `${labels.slice(0, -1).join(', ')} and ${labels[labels.length - 1]} are required.`;
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
