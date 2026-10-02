// src/components/environments/VaultCertificateRow.test.tsx

import { type as osType } from '@tauri-apps/plugin-os';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { VaultCertificateRow } from '@/components/environments/VaultCertificateRow';
import type {
  ClientCertificate,
  ExternalSecretBinding,
  VaultCertificateSummary,
} from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listVaultCertificates: vi.fn() };
});

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {
    // No-op for test polyfill.
  };
}

type VaultCert = Extract<ClientCertificate, { type: 'vault' }>;

const bindings: ExternalSecretBinding[] = [
  { alias: 'prod', connectionId: 'conn-1', vaultName: 'prod-vault', secretNames: [] },
  { alias: 'staging', connectionId: 'conn-2', vaultName: 'stage-vault', secretNames: [] },
];

const listed: VaultCertificateSummary[] = [
  {
    id: '1',
    name: 'client-a',
    exportable: true,
    enabled: true,
    keyAlgorithm: 'RSA-2048',
    expiresAt: null,
  },
  {
    id: '2',
    name: 'locked',
    exportable: false,
    enabled: true,
    keyAlgorithm: 'RSA-4096',
    expiresAt: null,
  },
  {
    id: '3',
    name: 'edge',
    exportable: true,
    enabled: true,
    keyAlgorithm: 'EC-P256',
    expiresAt: null,
  },
];

function renderRow(overrides: Partial<VaultCert> = {}) {
  const onChange = vi.fn();
  const cert: VaultCert = {
    type: 'vault',
    domain: 'api.example.com',
    binding: 'prod',
    certificate: 'client-a',
    format: 'pem',
    ...overrides,
  };
  const view = render(
    <VaultCertificateRow idx={0} cert={cert} bindings={bindings} onChange={onChange} />,
  );
  return { onChange, ...view };
}

const certificatePicker = () =>
  screen.getByRole('combobox', { name: 'Vault certificate for certificate 1' });

describe('VaultCertificateRow', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listVaultCertificates).mockReset().mockResolvedValue(listed);
    vi.mocked(osType).mockReturnValue('linux');
  });

  it('loads the certificates of the chosen binding', async () => {
    renderRow();
    await waitFor(() =>
      expect(tauriApi.listVaultCertificates).toHaveBeenCalledWith('conn-1', 'prod-vault'),
    );
    await waitFor(() => expect(certificatePicker()).toHaveTextContent('client-a · RSA-2048'));
  });

  // Review Focus 5.
  it('lists a non-exportable certificate as disabled with its key algorithm', async () => {
    renderRow({ certificate: '' });
    const user = userEvent.setup();
    await waitFor(() => expect(tauriApi.listVaultCertificates).toHaveBeenCalled());
    await user.click(certificatePicker());
    const locked = await screen.findByRole('option', {
      name: 'locked · RSA-4096 · not exportable',
    });
    expect(locked).toHaveAttribute('aria-disabled', 'true');
    expect(screen.getByRole('option', { name: 'client-a · RSA-2048' })).not.toHaveAttribute(
      'aria-disabled',
      'true',
    );
  });

  // Review Focus 2.
  it('shows a stored name that is no longer in the vault as not found', async () => {
    renderRow({ certificate: 'gone' });
    expect(
      await screen.findByText('Certificate gone was not found in this vault.'),
    ).toBeInTheDocument();
    expect(certificatePicker()).toHaveTextContent('gone (not found)');
  });

  // Review Focus 5.
  it('warns about an EC certificate with PEM on Windows only', async () => {
    vi.mocked(osType).mockReturnValue('windows');
    const pemOnWindows = renderRow({ certificate: 'edge', format: 'pem' });
    expect(await screen.findByText(/has an EC key/)).toBeInTheDocument();
    pemOnWindows.unmount();

    const pkcs12OnWindows = renderRow({ certificate: 'edge', format: 'pkcs12' });
    await waitFor(() => expect(certificatePicker()).toHaveTextContent('edge · EC-P256'));
    expect(screen.queryByText(/has an EC key/)).not.toBeInTheDocument();
    pkcs12OnWindows.unmount();

    vi.mocked(osType).mockReturnValue('linux');
    renderRow({ certificate: 'edge', format: 'pem' });
    await waitFor(() => expect(certificatePicker()).toHaveTextContent('edge · EC-P256'));
    expect(screen.queryByText(/has an EC key/)).not.toBeInTheDocument();
  });

  it('changing the binding clears the certificate', async () => {
    const { onChange } = renderRow();
    const user = userEvent.setup();
    await user.click(screen.getByRole('combobox', { name: 'Binding for certificate 1' }));
    await user.click(await screen.findByRole('option', { name: 'staging' }));
    expect(onChange).toHaveBeenCalledWith(0, { binding: 'staging', certificate: '' });
  });

  it('picks a certificate and a format, with PEM as the default', async () => {
    const { onChange } = renderRow({ certificate: '', format: undefined });
    const user = userEvent.setup();
    expect(screen.getByRole('combobox', { name: 'Format for certificate 1' })).toHaveTextContent(
      'PEM',
    );

    await user.click(certificatePicker());
    await user.click(await screen.findByRole('option', { name: 'client-a · RSA-2048' }));
    expect(onChange).toHaveBeenCalledWith(0, { certificate: 'client-a' });

    await user.click(screen.getByRole('combobox', { name: 'Format for certificate 1' }));
    await user.click(await screen.findByRole('option', { name: 'PKCS12' }));
    expect(onChange).toHaveBeenCalledWith(0, { format: 'pkcs12' });
  });

  it('shows the error when the list cannot be loaded', async () => {
    vi.mocked(tauriApi.listVaultCertificates).mockRejectedValue(
      'HTTP error: RocketVault rejected the access token (401).',
    );
    renderRow();
    expect(await screen.findByText(/Could not list certificates: .*\(401\)/)).toBeInTheDocument();
  });

  it('reloads the list on demand', async () => {
    renderRow();
    const reload = screen.getByRole('button', {
      name: 'Reload vault certificates for certificate 1',
    });
    await waitFor(() => expect(reload).toBeEnabled());
    await userEvent.setup().click(reload);
    await waitFor(() => expect(tauriApi.listVaultCertificates).toHaveBeenCalledTimes(2));
  });

  it('marks a binding that is not in the environment and lists nothing', async () => {
    renderRow({ binding: 'payments' });
    expect(
      await screen.findByText(
        'Binding payments is not in this environment. Pick one from the list.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Binding for certificate 1' })).toHaveTextContent(
      'payments (not in this environment)',
    );
    expect(tauriApi.listVaultCertificates).not.toHaveBeenCalled();
  });

  it('drops a list answer that arrives after the binding changed', async () => {
    const resolvers: Record<string, (list: VaultCertificateSummary[]) => void> = {};
    vi.mocked(tauriApi.listVaultCertificates).mockImplementation(
      (connectionId: string) =>
        new Promise<VaultCertificateSummary[]>((resolve) => {
          resolvers[connectionId] = resolve;
        }),
    );
    const onChange = vi.fn();
    const base: VaultCert = {
      type: 'vault',
      domain: 'api.example.com',
      binding: 'prod',
      certificate: 'client-a',
      format: 'pem',
    };
    const view = render(
      <VaultCertificateRow idx={0} cert={base} bindings={bindings} onChange={onChange} />,
    );
    await waitFor(() => expect(tauriApi.listVaultCertificates).toHaveBeenCalledTimes(1));

    view.rerender(
      <VaultCertificateRow
        idx={0}
        cert={{ ...base, binding: 'staging' }}
        bindings={bindings}
        onChange={onChange}
      />,
    );
    await waitFor(() => expect(tauriApi.listVaultCertificates).toHaveBeenCalledTimes(2));

    const listB = [{ ...listed[0], keyAlgorithm: 'RSA-4096' }];
    await act(async () => {
      resolvers['conn-2'](listB);
    });
    await waitFor(() => expect(certificatePicker()).toHaveTextContent('client-a · RSA-4096'));

    await act(async () => {
      resolvers['conn-1'](listed);
    });
    expect(certificatePicker()).toHaveTextContent('client-a · RSA-4096');
    expect(certificatePicker()).not.toHaveTextContent('RSA-2048');
  });
});
