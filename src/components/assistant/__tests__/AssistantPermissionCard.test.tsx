import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import type { AgentPermission } from '@/stores/assistant-store';
import { AssistantPermissionCard } from '../AssistantPermissionCard';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  respondAgentPermission: vi.fn(),
}));

function makePermission(overrides: Partial<AgentPermission> = {}): AgentPermission {
  return {
    requestId: 'perm-1',
    callId: 'call-1',
    title: 'propose_changes',
    kind: 'other',
    summary: '{"changes":[]}',
    status: 'pending',
    options: [
      {
        id: 'proceed_always_server',
        name: 'Allow all server tools for this session',
        kind: 'allow_always',
      },
      { id: 'proceed_once', name: 'Allow once', kind: 'allow_once' },
      { id: 'cancel', name: 'Deny', kind: 'reject_once' },
    ],
    ...overrides,
  };
}

describe('AssistantPermissionCard', () => {
  beforeEach(() => {
    vi.mocked(api.respondAgentPermission).mockReset();
    vi.mocked(api.respondAgentPermission).mockResolvedValue(undefined);
  });

  it('shows the tool, the summary and one button per offered option', () => {
    render(<AssistantPermissionCard sessionId='s1' permission={makePermission()} />);
    expect(screen.getByText('propose_changes')).toBeInTheDocument();
    expect(screen.getByText('{"changes":[]}')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Allow all server tools for this session' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Allow once' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Deny' })).toBeInTheDocument();
    expect(screen.getByText(/waiting for your answer/i)).toBeInTheDocument();
  });

  it('sends the chosen option id', async () => {
    render(<AssistantPermissionCard sessionId='s1' permission={makePermission()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Allow once' }));
    expect(api.respondAgentPermission).toHaveBeenCalledWith('s1', 'perm-1', 'proceed_once');
  });

  it('shows a failure and keeps the buttons clickable', async () => {
    vi.mocked(api.respondAgentPermission).mockRejectedValue(new Error('refused'));
    render(<AssistantPermissionCard sessionId='s1' permission={makePermission()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Deny' }));
    await waitFor(() => expect(screen.getByRole('alert')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Deny' })).toBeEnabled();
  });

  it('shows a caution badge for a command or an edit', () => {
    render(
      <AssistantPermissionCard sessionId='s1' permission={makePermission({ kind: 'execute' })} />,
    );
    expect(screen.getByText('Runs a command')).toBeInTheDocument();
  });

  it('keeps the full name of a long option and of an unknown kind', () => {
    const long = 'Allow every tool of the server named some-very-long-server for this whole session';
    render(
      <AssistantPermissionCard
        sessionId='s1'
        permission={makePermission({
          options: [
            { id: 'a', name: long, kind: 'allow_always' },
            // biome-ignore lint/suspicious/noExplicitAny: simulates a kind the app does not know.
            { id: 'b', name: 'Do something else', kind: 'brand_new' as any },
          ],
        })}
      />,
    );
    expect(screen.getByRole('button', { name: long })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Do something else' })).toBeInTheDocument();
  });

  it('renders the title and summary as plain text', () => {
    const { container } = render(
      <AssistantPermissionCard
        sessionId='s1'
        permission={makePermission({
          title: '<b>rm</b> **-rf**',
          summary: '[x](https://evil.test) <img src=x>',
        })}
      />,
    );
    expect(screen.getByText('<b>rm</b> **-rf**')).toBeInTheDocument();
    expect(screen.getByText('[x](https://evil.test) <img src=x>')).toBeInTheDocument();
    expect(container.querySelector('b, a, img')).toBeNull();
  });

  it('collapses to one line with the chosen option once answered', () => {
    render(
      <AssistantPermissionCard
        sessionId='s1'
        permission={makePermission({ status: 'answered', chosenName: 'Allow once' })}
      />,
    );
    expect(screen.getByText(/Allow once/)).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('shows a cancelled request as cancelled, without buttons', () => {
    render(
      <AssistantPermissionCard sessionId='s1' permission={makePermission({ status: 'cancelled' })} />,
    );
    expect(screen.getByText(/Cancelled/)).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});
