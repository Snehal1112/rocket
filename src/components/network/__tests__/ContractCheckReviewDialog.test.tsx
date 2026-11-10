import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ContractCheckReviewDialog } from '@/components/network/ContractCheckReviewDialog';
import { contractCheckToastId } from '@/lib/network-contract-check-utils';
import type { ContractCheckReview } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn(), dismiss: vi.fn() },
  api: {
    reviewContractCheck: vi.fn(),
    replyContractCheck: vi.fn(),
    declineContractCheck: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));
vi.mock('@/components/git/DiffViewer', () => ({
  DiffViewer: (props: { diffState: { filePath: string; oldContent: string; newContent: string } }) => (
    <div data-testid='diff' data-path={props.diffState.filePath}>
      <pre data-testid='diff-old'>{props.diffState.oldContent}</pre>
      <pre data-testid='diff-new'>{props.diffState.newContent}</pre>
    </div>
  ),
}));

const store = () => useNetworkStore.getState();

function makeReview(contract = 'Orders API'): ContractCheckReview {
  return {
    checkId: 'c1',
    deviceId: 'd1',
    contract,
    version: '2',
    policy: 'strict',
    changes: [
      {
        index: 0,
        method: 'GET',
        pathTemplate: '/orders/{id}',
        field: 'response.total',
        changeType: 'changed',
        oldValue: 'integer',
        newValue: 'string',
        breaking: true,
        verdict: 'break',
        matches: [{ collection: 'Shop', name: 'Get order' }],
      },
      {
        index: 1,
        method: 'POST',
        pathTemplate: '/orders',
        field: 'header.x-key',
        changeType: 'added',
        oldValue: null,
        newValue: '[hidden]',
        breaking: false,
        verdict: 'not_used',
        matches: [],
      },
    ],
  };
}

function openCheck(contract = 'Orders API') {
  mocks.api.reviewContractCheck.mockResolvedValue(makeReview(contract));
  store().addIncomingContractCheck({ checkId: 'c1', deviceId: 'd1', contract, changeCount: 2 });
  store().openContractCheckDialog('c1');
  return render(<ContractCheckReviewDialog />);
}

describe('ContractCheckReviewDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.replyContractCheck.mockResolvedValue(undefined);
    mocks.api.declineContractCheck.mockResolvedValue(undefined);
  });

  it('lists matches, verdict badges and the diff of each change', async () => {
    openCheck();
    expect(await screen.findByText('response.total')).toBeInTheDocument();
    expect(mocks.api.reviewContractCheck).toHaveBeenCalledWith('c1');
    expect(screen.getByText('Get order')).toBeInTheDocument();
    expect(screen.getByText('Break')).toBeInTheDocument();
    expect(screen.getByText('Not used')).toBeInTheDocument();
    const diffs = screen.getAllByTestId('diff');
    expect(diffs).toHaveLength(2);
    expect(screen.getAllByTestId('diff-old')[0]).toHaveTextContent('integer');
    expect(screen.getAllByTestId('diff-new')[0]).toHaveTextContent('string');
    expect(screen.getAllByTestId('diff-new')[1]).toHaveTextContent('[hidden]');
  });

  it('sends the result once, then closes and dismisses the toast', async () => {
    openCheck();
    await screen.findByText('response.total');
    await userEvent.click(screen.getByRole('button', { name: 'Send result' }));
    await waitFor(() => expect(store().dialogContractCheckId).toBeNull());
    expect(mocks.api.replyContractCheck).toHaveBeenCalledTimes(1);
    expect(mocks.api.replyContractCheck).toHaveBeenCalledWith('c1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith(contractCheckToastId('c1'));
    expect(store().contractChecksIncoming).toHaveLength(0);
  });

  it('declines and closes', async () => {
    openCheck();
    await screen.findByText('response.total');
    await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
    await waitFor(() => expect(store().dialogContractCheckId).toBeNull());
    expect(mocks.api.declineContractCheck).toHaveBeenCalledWith('c1');
    expect(mocks.api.replyContractCheck).not.toHaveBeenCalled();
  });

  it('disables both buttons after the check expired', async () => {
    openCheck();
    await screen.findByText('response.total');
    store().expireContractCheck('c1');
    expect(await screen.findByText('This check expired.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send result' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Decline' })).toBeDisabled();
  });

  it('renders a contract name with markup as plain text', async () => {
    const { baseElement } = openCheck('<script>alert(1)</script>');
    expect(await screen.findByText(/<script>alert\(1\)<\/script>/)).toBeInTheDocument();
    expect(baseElement.querySelector('script')).toBeNull();
  });

  it('closes on Escape without replying', async () => {
    openCheck();
    await screen.findByText('response.total');
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(store().dialogContractCheckId).toBeNull());
    expect(mocks.api.replyContractCheck).not.toHaveBeenCalled();
    expect(mocks.api.declineContractCheck).not.toHaveBeenCalled();
    expect(store().contractChecksIncoming).toHaveLength(1);
  });

  it('shows a fixed sentence and disables the buttons when the review is rejected', async () => {
    mocks.api.reviewContractCheck.mockRejectedValue(new Error('secret local request'));
    store().addIncomingContractCheck({ checkId: 'c1', deviceId: 'd1', contract: 'X', changeCount: 1 });
    store().openContractCheckDialog('c1');
    render(<ContractCheckReviewDialog />);
    expect(await screen.findByText('The check could not be loaded.')).toBeInTheDocument();
    expect(screen.queryByText(/secret local request/)).toBeNull();
    expect(screen.getByRole('button', { name: 'Send result' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Decline' })).toBeDisabled();
  });

  it('keeps the toast and re-enables the buttons when the reply fails', async () => {
    mocks.api.replyContractCheck.mockRejectedValue(new Error('raw backend text'));
    openCheck();
    await screen.findByText('response.total');
    await userEvent.click(screen.getByRole('button', { name: 'Send result' }));
    expect(await screen.findByText('Could not send. Try again.')).toBeInTheDocument();
    expect(screen.queryByText(/raw backend text/)).toBeNull();
    expect(screen.getByRole('button', { name: 'Send result' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Decline' })).toBeEnabled();
    expect(mocks.toast.dismiss).not.toHaveBeenCalled();
    expect(store().contractChecksIncoming).toHaveLength(1);
  });

  it('sends once on a double click', async () => {
    openCheck();
    await screen.findByText('response.total');
    const button = screen.getByRole('button', { name: 'Send result' });
    await userEvent.dblClick(button);
    await waitFor(() => expect(store().dialogContractCheckId).toBeNull());
    expect(mocks.api.replyContractCheck).toHaveBeenCalledTimes(1);
  });
});
