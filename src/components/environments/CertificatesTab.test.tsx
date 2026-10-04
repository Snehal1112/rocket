// src/components/environments/CertificatesTab.test.tsx

import { open } from '@tauri-apps/plugin-dialog';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CertificatesTab } from '@/components/environments/CertificatesTab';
import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';

// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    placeholder,
    isSecret,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
    isSecret?: boolean;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={label}
      placeholder={placeholder}
      data-secret={isSecret ? 'true' : 'false'}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listVaultCertificates: vi.fn().mockResolvedValue([]) };
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

const bindings: ExternalSecretBinding[] = [
  {
    alias: 'vault',
    connectionId: 'conn-1',
    vaultName: 'prod-vault',
    secretNames: [
      { name: 'clientCertPem', secretId: '1' },
      { name: 'clientKeyPass', secretId: '2' },
    ],
  },
];

const pem: ClientCertificate = {
  type: 'pem',
  domain: 'api.example.com',
  certificateFilePath: 'certs/a.pem',
  privateKeyFilePath: 'certs/a.key',
};

const pkcs12: ClientCertificate = {
  type: 'pkcs12',
  domain: '*.example.com',
  pkcs12FilePath: 'certs/b.p12',
};

function renderTab(
  certificates: ClientCertificate[],
  overrides: Partial<{
    isDirty: boolean;
    onSave: () => void;
    canAddVaultCertificate: boolean;
  }> = {},
) {
  const handlers = {
    onChange: vi.fn(),
    onAdd: vi.fn(),
    onRemove: vi.fn(),
    onMove: vi.fn(),
    onSave: overrides.onSave ?? vi.fn(),
  };
  render(
    <CertificatesTab
      certificates={certificates}
      bindings={bindings}
      {...handlers}
      isDirty={overrides.isDirty ?? false}
      saveState='idle'
      canAddVaultCertificate={overrides.canAddVaultCertificate}
    />,
  );
  return handlers;
}

describe('CertificatesTab', () => {
  beforeEach(() => {
    vi.mocked(open).mockReset();
  });

  it('shows the RocketVault certificate button by default', () => {
    renderTab([]);
    expect(
      screen.getByRole('button', { name: /add rocketvault certificate/i }),
    ).toBeInTheDocument();
  });

  it('hides the RocketVault certificate button when no binding can supply certificates', () => {
    renderTab([], { canAddVaultCertificate: false });
    expect(
      screen.queryByRole('button', { name: /add rocketvault certificate/i }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add PEM' })).toBeInTheDocument();
  });

  it('shows an empty state and the add buttons when there are no certificates', () => {
    renderTab([]);
    expect(screen.getByText('No client certificates')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add PEM' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add PKCS12' })).toBeInTheDocument();
  });

  it('shows a type badge, the domain and the hints for each row', () => {
    renderTab([pem, pkcs12]);
    expect(screen.getByText('PEM')).toBeInTheDocument();
    expect(screen.getByText('PKCS12')).toBeInTheDocument();
    expect(screen.getByLabelText('Domain for certificate 1')).toHaveValue('api.example.com');
    expect(screen.getByLabelText('Domain for certificate 2')).toHaveValue('*.example.com');
    expect(screen.getByText(/wildcard/i)).toBeInTheDocument();
    expect(screen.getByText(/Relative paths start at the collection folder/)).toBeInTheDocument();
    expect(screen.getByText(/Encrypted PEM keys need their passphrase/)).toBeInTheDocument();
  });

  it('adds a PEM and a PKCS12 certificate', async () => {
    const { onAdd } = renderTab([pem]);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Add PEM' }));
    await user.click(screen.getByRole('button', { name: 'Add PKCS12' }));
    expect(onAdd).toHaveBeenNthCalledWith(1, 'pem');
    expect(onAdd).toHaveBeenNthCalledWith(2, 'pkcs12');
  });

  it('edits the domain and a file path', () => {
    const { onChange } = renderTab([pem]);
    fireEvent.change(screen.getByLabelText('Domain for certificate 1'), {
      target: { value: 'b.example.com:8443' },
    });
    expect(onChange).toHaveBeenLastCalledWith(0, { domain: 'b.example.com:8443' });
    fireEvent.change(screen.getByLabelText('Private key file path for certificate 1'), {
      target: { value: 'certs/new.key' },
    });
    expect(onChange).toHaveBeenLastCalledWith(0, { privateKeyFilePath: 'certs/new.key' });
  });

  it('removes the right certificate', async () => {
    const { onRemove } = renderTab([pem, pkcs12]);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Delete certificate 2' }));
    expect(onRemove).toHaveBeenCalledWith(1);
  });

  it('reorders certificates through onMove so the match order can change', async () => {
    const { onMove } = renderTab([pem, pkcs12]);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Move certificate 2 up' }));
    expect(onMove).toHaveBeenLastCalledWith(1, -1);
    await user.click(screen.getByRole('button', { name: 'Move certificate 1 down' }));
    expect(onMove).toHaveBeenLastCalledWith(0, 1);
    // The ends cannot move past the list.
    expect(screen.getByRole('button', { name: 'Move certificate 1 up' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Move certificate 2 down' })).toBeDisabled();
  });

  it('switching to a vault source clears the file path', async () => {
    const { onChange } = renderTab([pem]);
    const user = userEvent.setup();
    await user.click(
      screen.getByRole('combobox', { name: 'Certificate source for certificate 1' }),
    );
    await user.click(await screen.findByRole('option', { name: 'Vault secret' }));
    expect(onChange).toHaveBeenCalledWith(0, { certificateFilePath: '', certificateSecret: '' });
  });

  it('switching to a file source clears the secret', async () => {
    const vaultPem: ClientCertificate = {
      type: 'pem',
      domain: 'api.example.com',
      certificateSecret: 'vault.clientCertPem',
      privateKeyFilePath: 'certs/a.key',
    };
    const { onChange } = renderTab([vaultPem]);
    const user = userEvent.setup();
    await user.click(
      screen.getByRole('combobox', { name: 'Certificate source for certificate 1' }),
    );
    await user.click(await screen.findByRole('option', { name: 'File' }));
    expect(onChange).toHaveBeenCalledWith(0, { certificateSecret: undefined });
  });

  it('picks a vault secret from the bound secrets', async () => {
    const vaultPem: ClientCertificate = {
      type: 'pem',
      domain: 'api.example.com',
      certificateSecret: '',
      privateKeyFilePath: 'certs/a.key',
    };
    const { onChange } = renderTab([vaultPem]);
    const user = userEvent.setup();
    await user.click(
      screen.getByRole('combobox', { name: 'Certificate vault secret for certificate 1' }),
    );
    await user.click(await screen.findByRole('option', { name: 'vault.clientCertPem' }));
    expect(onChange).toHaveBeenCalledWith(0, { certificateSecret: 'vault.clientCertPem' });
  });

  it('keeps a saved reference visible when it is not in the fetched names', async () => {
    const stale: ClientCertificate = {
      type: 'pkcs12',
      domain: 'a.com',
      pkcs12Secret: 'vault.gone',
    };
    renderTab([stale]);
    const trigger = screen.getByRole('combobox', {
      name: 'PKCS12 bundle vault secret for certificate 1',
    });
    expect(trigger).toHaveTextContent('vault.gone (not fetched)');
  });

  it('browses for a file with the native picker', async () => {
    vi.mocked(open).mockResolvedValue('/home/user/client.pem');
    const { onChange } = renderTab([pem]);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Browse for certificate file for certificate 1' }));
    await waitFor(() =>
      expect(onChange).toHaveBeenCalledWith(0, { certificateFilePath: '/home/user/client.pem' }),
    );
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ multiple: false }));
  });

  it('does not change anything when the picker is cancelled', async () => {
    vi.mocked(open).mockResolvedValue(null);
    const { onChange } = renderTab([pem]);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Browse for certificate file for certificate 1' }));
    await waitFor(() => expect(open).toHaveBeenCalled());
    expect(onChange).not.toHaveBeenCalled();
  });

  it('masks the passphrase field and edits the passphrase', () => {
    const { onChange } = renderTab([{ ...pem, passphrase: 'abc' }]);
    const field = screen.getByLabelText('Passphrase for certificate 1');
    expect(field).toHaveAttribute('data-secret', 'true');
    fireEvent.change(field, { target: { value: 'abcd' } });
    expect(onChange).toHaveBeenLastCalledWith(0, { passphrase: 'abcd' });
    fireEvent.change(field, { target: { value: '' } });
    expect(onChange).toHaveBeenLastCalledWith(0, { passphrase: undefined });
  });

  it('inserts a vault placeholder into the passphrase', async () => {
    const { onChange } = renderTab([pem]);
    const user = userEvent.setup();
    await user.click(
      screen.getByRole('combobox', {
        name: 'Insert vault secret into passphrase for certificate 1',
      }),
    );
    await user.click(await screen.findByRole('option', { name: 'vault.clientKeyPass' }));
    expect(onChange).toHaveBeenCalledWith(0, { passphrase: '{{vault.clientKeyPass}}' });
  });

  it('warns about a literal passphrase and stays quiet for a placeholder or none', () => {
    renderTab([
      { ...pem, passphrase: 'hunter2' },
      { ...pkcs12, passphrase: '{{vault.clientKeyPass}}' },
    ]);
    expect(screen.getAllByText(/saved in the environment file/)).toHaveLength(1);
  });

  it('enables Save only when dirty and calls onSave', async () => {
    const onSave = vi.fn();
    renderTab([pem], { isDirty: true, onSave });
    await userEvent.setup().click(screen.getByRole('button', { name: /^save$/i }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it('disables Save when nothing changed', () => {
    renderTab([pem], { isDirty: false });
    expect(screen.getByRole('button', { name: /^save$/i })).toBeDisabled();
  });

  it('adds a RocketVault certificate', async () => {
    const { onAdd } = renderTab([]);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Add RocketVault certificate' }));
    expect(onAdd).toHaveBeenCalledWith('vault');
  });

  it('renders a vault row with its pickers and no file or passphrase fields', async () => {
    renderTab([
      {
        type: 'vault',
        domain: 'api.example.com',
        binding: 'vault',
        certificate: 'client-a',
        format: 'pem',
      },
    ]);
    expect(screen.getByText('Vault')).toBeInTheDocument();
    expect(screen.getByLabelText('Domain for certificate 1')).toHaveValue('api.example.com');
    expect(screen.getByRole('combobox', { name: 'Binding for certificate 1' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Format for certificate 1' })).toHaveTextContent(
      'PEM',
    );
    expect(screen.queryByLabelText('Passphrase for certificate 1')).not.toBeInTheDocument();
    expect(
      screen.queryByRole('combobox', { name: 'Certificate source for certificate 1' }),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(tauriApi.listVaultCertificates).toHaveBeenCalledWith('conn-1', 'prod-vault'),
    );
  });
});
