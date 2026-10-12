import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import type { ToolMessage } from '@/lib/assistant/timeline';
import { ToolRun, toolName } from '../ToolRun';

const t = (id: string, status: ToolMessage['status'], title = `get_${id}(x)`): ToolMessage => ({
  kind: 'tool',
  id,
  callId: id,
  title,
  status,
});

describe('ToolRun', () => {
  it('summarises finished tools, collapsed by default', () => {
    render(<ToolRun tools={[t('a', 'completed'), t('b', 'completed')]} />);
    expect(screen.getByRole('button', { name: /Used 2 tools/ })).toBeInTheDocument();
    expect(screen.queryByText('get_a(x)')).toBeNull();
  });

  it('uses the singular for one tool', () => {
    render(<ToolRun tools={[t('a', 'completed')]} />);
    expect(screen.getByText('Used 1 tool')).toBeInTheDocument();
  });

  it('shows the latest running tool name without arguments', () => {
    render(<ToolRun tools={[t('a', 'completed'), t('b', 'in_progress'), t('c', 'pending')]} />);
    expect(screen.getByText('Working: get_c')).toBeInTheDocument();
  });

  it('shows the failed count when done and while running', () => {
    const { rerender } = render(<ToolRun tools={[t('a', 'failed'), t('b', 'completed')]} />);
    expect(screen.getByText('· 1 failed', { exact: false })).toBeInTheDocument();
    rerender(<ToolRun tools={[t('a', 'failed'), t('b', 'in_progress')]} />);
    expect(screen.getByText('Working: get_b')).toBeInTheDocument();
    expect(screen.getByText('· 1 failed', { exact: false })).toBeInTheDocument();
  });

  it('expands to one row per tool with status text and a title attribute', async () => {
    render(<ToolRun tools={[t('a', 'completed'), t('b', 'failed')]} />);
    await userEvent.click(screen.getByRole('button'));
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
    expect(screen.getByText('get_a(x)')).toHaveAttribute('title', 'get_a(x)');
    expect(screen.getByText('done')).toBeInTheDocument();
    expect(screen.getByText('failed', { selector: '.sr-only' })).toBeInTheDocument();
  });

  it('extracts the function name', () => {
    expect(toolName('run_request(id=1)')).toBe('run_request');
    expect(toolName('get_request: login')).toBe('get_request');
    expect(toolName('Reading GET /orders')).toBe('Reading GET /orders');
  });
});
