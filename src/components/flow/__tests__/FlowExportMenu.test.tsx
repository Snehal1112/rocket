import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import type { FlowTab } from '@/types/pane-types';

const clip = vi.hoisted(() => ({ copyTextAsync: vi.fn() }));
const files = vi.hoisted(() => ({ saveTextFile: vi.fn() }));
vi.mock('@/lib/clipboard', () => clip);
vi.mock('@/lib/save-file', () => files);
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

import { FlowExportMenu } from '../FlowExportMenu';

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

const node = (id: string, kind: FlowNode['kind']): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

const tab = (over: Partial<FlowTab> = {}): FlowTab => ({
  id: 't1',
  title: 'Flow: my flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my flow',
  nodes: [
    node('a1', {
      kind: 'Auth',
      label: 'Sign in',
      auth: { authType: 'bearer', token: 'canary-menu-token-1' },
      applyToInherit: true,
    }),
    node('o1', { kind: 'Output', label: 'Out' }),
  ],
  edges: [],
  nodeStatus: { a1: 'success', o1: 'success' },
  nodeDetail: { a1: {}, o1: { value: 'done' } },
  runState: 'done',
  runId: 'run-9',
  ...over,
});

async function openMenu() {
  await userEvent.click(screen.getByRole('button', { name: 'Export' }));
}

describe('FlowExportMenu', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clip.copyTextAsync.mockResolvedValue(undefined);
    files.saveTextFile.mockResolvedValue(true);
  });

  it('lists the three actions', async () => {
    render(<FlowExportMenu tab={tab()} />);
    await openMenu();
    expect(await screen.findByRole('menuitem', { name: 'Copy flow JSON' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Export run report (JSON)' })).toBeInTheDocument();
    expect(
      screen.getByRole('menuitem', { name: 'Export run report (Markdown)' }),
    ).toBeInTheDocument();
  });

  it('disables the report items until a run has finished', async () => {
    const { rerender } = render(<FlowExportMenu tab={tab({ runState: 'idle', nodeDetail: undefined })} />);
    await openMenu();
    expect(await screen.findByRole('menuitem', { name: 'Export run report (JSON)' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    expect(screen.getByRole('menuitem', { name: 'Copy flow JSON' })).not.toHaveAttribute(
      'aria-disabled',
      'true',
    );

    rerender(<FlowExportMenu tab={tab({ runState: 'running' })} />);
    expect(screen.getByRole('menuitem', { name: 'Export run report (JSON)' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
  });

  it('copies the masked flow, handing the clipboard a promise inside the click', async () => {
    render(<FlowExportMenu tab={tab()} />);
    await openMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Copy flow JSON' }));

    expect(clip.copyTextAsync).toHaveBeenCalledTimes(1);
    const arg = clip.copyTextAsync.mock.calls[0][0] as Promise<string>;
    expect(arg).toBeInstanceOf(Promise);
    const text = await arg;
    expect(text).not.toContain('canary-menu-token-1');
    expect(JSON.parse(text).name).toBe('my flow');
    expect(toast.success).toHaveBeenCalledWith('Flow JSON copied. 1 secret masked.');
  });

  it('copies without a count when nothing was masked', async () => {
    const clean = tab({
      nodes: [node('o1', { kind: 'Output', label: 'Out' })],
      nodeStatus: { o1: 'success' },
    });
    render(<FlowExportMenu tab={clean} />);
    await openMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Copy flow JSON' }));
    await act(async () => {});
    expect(toast.success).toHaveBeenCalledWith('Flow JSON copied.');
  });

  it('reports a failed copy as an error', async () => {
    clip.copyTextAsync.mockRejectedValue(new Error('no clipboard'));
    render(<FlowExportMenu tab={tab()} />);
    await openMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Copy flow JSON' }));
    await act(async () => {});
    expect(toast.error).toHaveBeenCalled();
    expect(toast.success).not.toHaveBeenCalled();
  });

  it('saves the JSON report under a safe file name', async () => {
    render(<FlowExportMenu tab={tab()} />);
    await openMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Export run report (JSON)' }));

    expect(files.saveTextFile).toHaveBeenCalledTimes(1);
    const [name, text, filters] = files.saveTextFile.mock.calls[0];
    expect(name).toBe('my-flow-run-report.json');
    expect(filters).toEqual([{ name: 'JSON', extensions: ['json'] }]);
    expect(JSON.parse(text).runId).toBe('run-9');
    expect(toast.success).toHaveBeenCalledWith('Run report saved.');
  });

  it('saves the Markdown report', async () => {
    render(<FlowExportMenu tab={tab()} />);
    await openMenu();
    await userEvent.click(
      await screen.findByRole('menuitem', { name: 'Export run report (Markdown)' }),
    );
    const [name, text, filters] = files.saveTextFile.mock.calls[0];
    expect(name).toBe('my-flow-run-report.md');
    expect(filters).toEqual([{ name: 'Markdown', extensions: ['md'] }]);
    expect(text).toContain('# Run report: my flow');
  });

  it('shows no success message when the save dialog is cancelled', async () => {
    files.saveTextFile.mockResolvedValue(false);
    render(<FlowExportMenu tab={tab()} />);
    await openMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Export run report (JSON)' }));
    await act(async () => {});
    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('shows an error when the file cannot be written', async () => {
    files.saveTextFile.mockRejectedValue(new Error('disk full'));
    render(<FlowExportMenu tab={tab()} />);
    await openMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Export run report (JSON)' }));
    await act(async () => {});
    expect(toast.error).toHaveBeenCalledWith('Could not save the report: Error: disk full');
    expect(toast.success).not.toHaveBeenCalled();
  });
});
