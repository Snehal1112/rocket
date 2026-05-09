import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NewContractModal } from './NewContractModal';

const mockCreate = vi.fn().mockResolvedValue({ id: 'new', status: 'active' });
const mockRecompute = vi.fn().mockResolvedValue([]);

vi.mock('@/stores/contracts/contractsSlice', () => ({
  useContractsStore: (selector: (s: unknown) => unknown) =>
    selector({ createContract: mockCreate, recomputeDrift: mockRecompute }),
}));

// sonner toast is a no-op in tests
vi.mock('sonner', () => ({
  toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn() }),
}));

function renderModal(props = {}) {
  return render(
    <NewContractModal
      open={true}
      onOpenChange={vi.fn()}
      collectionId='col1'
      collectionName='Payments'
      {...props}
    />,
  );
}

beforeEach(() => {
  mockCreate.mockClear();
  mockRecompute.mockClear();
});

describe('NewContractModal', () => {
  it('renders dialog title', () => {
    renderModal();
    expect(screen.getByText('New contract')).toBeInTheDocument();
  });

  it('default version is 0.1.0', () => {
    renderModal();
    expect(screen.getByDisplayValue('0.1.0')).toBeInTheDocument();
  });

  it('shows error when name is too short on submit', async () => {
    renderModal();
    fireEvent.click(screen.getByText('Create & Publish →'));
    await waitFor(() =>
      expect(screen.getByText('At least 2 characters')).toBeInTheDocument(),
    );
  });

  it('shows error when version is not semver', async () => {
    renderModal();
    const versionInput = screen.getByDisplayValue('0.1.0');
    await userEvent.clear(versionInput);
    await userEvent.type(versionInput, 'not-semver');
    fireEvent.click(screen.getByText('Create & Publish →'));
    await waitFor(() =>
      expect(screen.getByText('Must be semver, e.g. 1.0.0')).toBeInTheDocument(),
    );
  });

  it('shows error when consumers field is empty', async () => {
    renderModal();
    await userEvent.type(screen.getByPlaceholderText('Payments API v2'), 'My API');
    await userEvent.type(screen.getByPlaceholderText('Billing Team'), 'Billing');
    fireEvent.click(screen.getByText('Create & Publish →'));
    await waitFor(() =>
      expect(screen.getByText('At least one consumer required')).toBeInTheDocument(),
    );
  });

  it('calls createContract + recomputeDrift on valid publish', async () => {
    renderModal();
    await userEvent.type(screen.getByPlaceholderText('Payments API v2'), 'Payments v2');
    await userEvent.type(screen.getByPlaceholderText('Billing Team'), 'Billing');
    await userEvent.type(screen.getByPlaceholderText('Platform, Mobile'), 'Platform');
    fireEvent.click(screen.getByText('Create & Publish →'));
    await waitFor(() => expect(mockCreate).toHaveBeenCalledTimes(1));
    expect(mockCreate.mock.calls[0][1]).toMatchObject({
      name: 'Payments v2',
      publishImmediately: true,
    });
    expect(mockRecompute).toHaveBeenCalledWith('col1');
  });
});
