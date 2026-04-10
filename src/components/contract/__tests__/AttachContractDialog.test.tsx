import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { attachContract as apiAttach, type Contract, type ContractScope } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';
import { AttachContractDialog } from '../AttachContractDialog';

// vi.mock is hoisted above the import above, so the imported `apiAttach`
// resolves to the mocked function below.
vi.mock('@/lib/tauri-api', () => ({
  attachContract: vi.fn(),
  listContracts: vi.fn(),
  getContract: vi.fn(),
  deleteContract: vi.fn(),
  getContractChangelog: vi.fn(),
}));

const mockAttach = vi.mocked(apiAttach);

const ROOT = '/tmp/workspace/collections/payments';
const collectionScope: ContractScope = { type: 'collection' };

function resetStore() {
  useContractStore.setState({
    contractsByRoot: {},
    changelogs: {},
    error: null,
    loading: false,
  });
}

function makeReturnedContract(overrides: Partial<Contract> = {}): Contract {
  return {
    id: 'c1',
    title: 'Payments API v1',
    provider: 'Billing Team',
    consumer: 'Platform Team',
    project: 'Checkout',
    version: 'v1',
    effectiveDate: '2026-01-01',
    expiryDate: null,
    documentPath: null,
    enforcementMode: 'warn',
    scope: collectionScope,
    ...overrides,
  };
}

function fillValidForm() {
  fireEvent.change(screen.getByLabelText(/^title$/i), { target: { value: 'Payments API v1' } });
  fireEvent.change(screen.getByLabelText(/provider team/i), {
    target: { value: 'Billing Team' },
  });
  fireEvent.change(screen.getByLabelText(/consumer team/i), {
    target: { value: 'Platform Team' },
  });
  fireEvent.change(screen.getByLabelText(/^project$/i), { target: { value: 'Checkout' } });
  fireEvent.change(screen.getByLabelText(/^version$/i), { target: { value: 'v1' } });
  // Effective date is pre-filled to today; nothing to change.
}

describe('AttachContractDialog', () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
  });

  it('renders the scope label from defaultScope', () => {
    render(
      <AttachContractDialog
        open
        onOpenChange={vi.fn()}
        collectionRoot={ROOT}
        defaultScope={collectionScope}
      />,
    );
    expect(screen.getByText(/entire collection/i)).toBeDefined();
  });

  it('shows a validation error when submitting with missing fields', () => {
    render(
      <AttachContractDialog
        open
        onOpenChange={vi.fn()}
        collectionRoot={ROOT}
        defaultScope={collectionScope}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /attach contract/i }));

    expect(screen.getByText(/all fields except expiry date are required/i)).toBeDefined();
    expect(mockAttach).not.toHaveBeenCalled();
  });

  it('submits a valid form and closes the dialog on success', async () => {
    mockAttach.mockResolvedValueOnce(makeReturnedContract());
    const onOpenChange = vi.fn();

    render(
      <AttachContractDialog
        open
        onOpenChange={onOpenChange}
        collectionRoot={ROOT}
        defaultScope={collectionScope}
      />,
    );

    fillValidForm();
    fireEvent.click(screen.getByRole('button', { name: /attach contract/i }));

    await waitFor(() => expect(mockAttach).toHaveBeenCalledTimes(1));
    const [rootArg, inputArg] = mockAttach.mock.calls[0];
    expect(rootArg).toBe(ROOT);
    expect(inputArg).toMatchObject({
      title: 'Payments API v1',
      provider: 'Billing Team',
      consumer: 'Platform Team',
      project: 'Checkout',
      version: 'v1',
      scope: collectionScope,
      initialSnapshots: [],
      expiryDate: null,
      documentPath: null,
    });
    // Effective date is today's ISO prefix — we only check the shape.
    expect(inputArg.effectiveDate).toMatch(/^\d{4}-\d{2}-\d{2}$/);

    // Dialog is closed on success.
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it('surfaces backend errors without closing the dialog', async () => {
    mockAttach.mockRejectedValueOnce(new Error('validation failed'));
    const onOpenChange = vi.fn();

    render(
      <AttachContractDialog
        open
        onOpenChange={onOpenChange}
        collectionRoot={ROOT}
        defaultScope={collectionScope}
      />,
    );

    fillValidForm();
    fireEvent.click(screen.getByRole('button', { name: /attach contract/i }));

    await waitFor(() => expect(mockAttach).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByText(/validation failed/i)).toBeDefined());
    // Dialog stays open so the user can retry — no close call.
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
  });

  it('cancel closes the dialog without calling the backend', () => {
    const onOpenChange = vi.fn();

    render(
      <AttachContractDialog
        open
        onOpenChange={onOpenChange}
        collectionRoot={ROOT}
        defaultScope={collectionScope}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /^cancel$/i }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(mockAttach).not.toHaveBeenCalled();
  });

  it('formats a folder scope label', () => {
    render(
      <AttachContractDialog
        open
        onOpenChange={vi.fn()}
        collectionRoot={ROOT}
        defaultScope={{ type: 'folder', rel_path: 'auth' }}
      />,
    );
    expect(screen.getByText(/folder: auth/i)).toBeDefined();
  });
});
