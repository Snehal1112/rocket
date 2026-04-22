import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MarkdownEditor } from './MarkdownEditor';

const baseProps = { value: '', onChange: vi.fn() };

beforeEach(() => {
  baseProps.onChange.mockClear();
});

describe('MarkdownEditor', () => {
  it('renders Documentation label', () => {
    render(<MarkdownEditor {...baseProps} />);
    expect(screen.getByText('Documentation')).toBeInTheDocument();
  });

  it('defaults to preview mode and shows empty state when value is empty', () => {
    render(<MarkdownEditor {...baseProps} />);
    expect(screen.getByText('No documentation yet')).toBeInTheDocument();
  });

  it('renders markdown content in preview mode when value is set', () => {
    render(<MarkdownEditor {...baseProps} value='# Hello' />);
    expect(screen.getByRole('heading', { name: 'Hello' })).toBeInTheDocument();
  });

  it('renders textarea when mode=edit is passed', () => {
    render(<MarkdownEditor {...baseProps} mode='edit' />);
    expect(screen.getByPlaceholderText(/Add documentation/)).toBeInTheDocument();
  });

  it('calls onChange when user types in edit mode', async () => {
    const onChange = vi.fn();
    render(<MarkdownEditor {...baseProps} mode='edit' onChange={onChange} />);
    await userEvent.type(screen.getByPlaceholderText(/Add documentation/), 'x');
    expect(onChange).toHaveBeenCalled();
  });

  it('calls onModeChange when Edit tab is clicked', async () => {
    const onModeChange = vi.fn();
    render(<MarkdownEditor {...baseProps} onModeChange={onModeChange} />);
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    expect(onModeChange).toHaveBeenCalledWith('edit');
  });

  it('does not render save button when onSave is not provided', () => {
    render(<MarkdownEditor {...baseProps} mode='edit' />);
    expect(screen.queryByRole('button', { name: /save/i })).not.toBeInTheDocument();
  });

  it('renders save button when onSave is provided', () => {
    render(<MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={false} />);
    expect(screen.getByRole('button', { name: /save/i })).toBeInTheDocument();
  });

  it('save button disabled when isDirty is false', () => {
    render(<MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={false} />);
    expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
  });

  it('save button enabled when isDirty is true', () => {
    render(<MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={true} />);
    expect(screen.getByRole('button', { name: /save/i })).toBeEnabled();
  });

  it('clicking Add Documentation calls onModeChange with edit', async () => {
    const onModeChange = vi.fn();
    render(<MarkdownEditor {...baseProps} onModeChange={onModeChange} />);
    await userEvent.click(screen.getByRole('button', { name: /add documentation/i }));
    expect(onModeChange).toHaveBeenCalledWith('edit');
  });
});
