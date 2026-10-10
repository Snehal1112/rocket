import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ConfigOption } from '@/lib/tauri-api';
import { ComposerToolbar, type ComposerToolbarProps } from '../ComposerToolbar';

// Radix menus call pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

const MODEL = {
  id: 'model',
  name: 'Model',
  currentValue: 'sonnet',
  choices: [
    { value: 'sonnet', name: 'Sonnet' },
    { value: 'opus', name: 'Opus' },
  ],
} as ConfigOption;
const EFFORT = {
  id: 'effort',
  name: 'Effort',
  currentValue: 'high',
  choices: [
    { value: 'low', name: 'Low' },
    { value: 'high', name: 'High' },
  ],
} as ConfigOption;

function setup(overrides: Partial<ComposerToolbarProps> = {}) {
  const props: ComposerToolbarProps = {
    mode: 'ask',
    onModeChange: vi.fn(),
    configOptions: [MODEL],
    onConfigChange: vi.fn(),
    usage: undefined,
    running: false,
    canSend: true,
    onSend: vi.fn(),
    onStop: vi.fn(),
    ...overrides,
  };
  render(<ComposerToolbar {...props} />);
  return { props, user: userEvent.setup() };
}

describe('ComposerToolbar', () => {
  it('changes the mode', async () => {
    const { props, user } = setup();
    await user.click(screen.getByRole('button', { name: 'Mode: Ask' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Edit/ }));
    expect(props.onModeChange).toHaveBeenCalledWith('edit');
  });

  it('explains each mode in the mode menu', async () => {
    // The summary line under the toolbar was removed; the menu describes each mode.
    const { user } = setup({ mode: 'agent' });
    await user.click(screen.getByRole('button', { name: 'Mode: Agent' }));
    expect(screen.getByText(/environment's credentials only/)).toBeInTheDocument();
  });

  it('changes the model through its config option', async () => {
    const { props, user } = setup();
    await user.click(screen.getByRole('button', { name: 'Model: Sonnet' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Opus/ }));
    expect(props.onConfigChange).toHaveBeenCalledWith('model', 'opus');
  });

  it('shows Effort only when the agent reports it', () => {
    setup();
    expect(screen.queryByRole('button', { name: /^Effort/ })).toBeNull();
  });

  it('shows Effort when present and hides Model when absent', () => {
    setup({ configOptions: [EFFORT] });
    expect(screen.getByRole('button', { name: 'Effort: High' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Model/ })).toBeNull();
  });

  it('disables the pickers with a reason when there is no session', () => {
    setup({ disabled: true, canSend: false });
    const mode = screen.getByRole('button', { name: 'Mode: Ask' });
    expect(mode).toBeDisabled();
    expect(mode).toHaveAttribute('title', 'Start a session to use this.');
  });

  it('disables the pickers while a change is applied', () => {
    setup({ changing: true });
    expect(screen.getByRole('button', { name: 'Mode: Ask' })).toBeDisabled();
  });

  it('shows the context used with tokens and cost for screen readers', () => {
    setup({ usage: { used: 12, size: 100, costUsd: 0.5 } });
    expect(screen.getByText('12%')).toBeInTheDocument();
    expect(screen.getByText(/12 of 100 tokens, \$0\.5000/)).toBeInTheDocument();
  });

  it('hides the context indicator for a zero size', () => {
    setup({ usage: { used: 0, size: 0 } });
    expect(screen.queryByText('0%')).toBeNull();
  });

  it('disables Send when there is nothing to send', () => {
    setup({ canSend: false });
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('turns Send into Stop while a turn runs', async () => {
    const { props, user } = setup({ running: true });
    expect(screen.queryByRole('button', { name: 'Send' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Stop' }));
    expect(props.onStop).toHaveBeenCalledTimes(1);
  });
});
