import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import { useCallback, useState } from 'react';
import type { SecretManagerConnection } from '@/lib/tauri-api';
import { AgentCredentialSection } from '../AgentCredentialSection';
import {
  type AgentKindValue,
  type CredentialErrors,
  type CredentialFormState,
  EMPTY_CREDENTIAL_FORM,
} from '../form-types';

// Radix Select calls browser APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.releasePointerCapture) {
  Element.prototype.releasePointerCapture = () => undefined;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

export const CONNECTIONS = [
  { id: 'rv', label: 'Prod vault', provider: 'rocketvault' },
  { id: 'az', label: 'Team Azure', provider: 'azure' },
  { id: 'aws1', label: 'Legacy AWS', provider: 'aws' },
] as SecretManagerConnection[];

interface Options {
  kind?: AgentKindValue;
  initial?: Partial<CredentialFormState>;
  errors?: CredentialErrors;
  hasSavedCredential?: boolean;
}

// Renders the section in a small stateful parent. `latest.current` holds the form state.
export function renderSection(options: Options = {}) {
  const latest: { current: CredentialFormState } = {
    current: { ...EMPTY_CREDENTIAL_FORM, ...options.initial },
  };
  function Harness() {
    const [value, setValue] = useState<CredentialFormState>(latest.current);
    const onChange = useCallback((patch: Partial<CredentialFormState>) => {
      latest.current = { ...latest.current, ...patch };
      setValue(latest.current);
    }, []);
    return (
      <AgentCredentialSection
        kind={options.kind ?? 'claude'}
        value={value}
        onChange={onChange}
        errors={options.errors ?? {}}
        hasSavedCredential={options.hasSavedCredential ?? false}
      />
    );
  }
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const utils = render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
  return { latest, ...utils };
}
