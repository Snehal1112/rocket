import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ScriptSnippetSidebar } from '../ScriptSnippetSidebar';

describe('ScriptSnippetSidebar', () => {
  it('renders Common Tests and API Reference section headings', () => {
    render(<ScriptSnippetSidebar onInsert={vi.fn()} />);
    expect(screen.getByText('Common Tests')).toBeTruthy();
    expect(screen.getByText('API Reference')).toBeTruthy();
  });

  it('calls onInsert with snippet code when a common-tests item is clicked', () => {
    const onInsert = vi.fn();
    render(<ScriptSnippetSidebar onInsert={onInsert} />);
    fireEvent.click(screen.getByText('Status is 200'));
    expect(onInsert).toHaveBeenCalledWith(
      expect.stringContaining('expect(res.getStatus()).to.equal(200)'),
    );
  });

  it('renders res, rok, expect sub-group labels inside API Reference', () => {
    render(<ScriptSnippetSidebar onInsert={vi.fn()} />);
    expect(screen.getByText('res.*')).toBeTruthy();
    expect(screen.getByText('rok.*')).toBeTruthy();
    expect(screen.getByText('expect')).toBeTruthy();
  });
});
