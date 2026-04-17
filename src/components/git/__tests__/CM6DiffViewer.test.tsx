import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CM6DiffViewer } from '../CM6DiffViewer';

describe('CM6DiffViewer', () => {
  it('renders without crashing', () => {
    const { container } = render(
      <CM6DiffViewer
        oldContent='line 1\nline 2'
        newContent='line 1\nline 2 modified'
        filePath='request.yml'
      />,
    );
    // MergeView creates a .cm-mergeView container.
    const mergeView = container.querySelector('.cm-mergeView');
    expect(mergeView).not.toBeNull();
  });

  it('renders two editor panes', () => {
    const { container } = render(
      <CM6DiffViewer oldContent='old content' newContent='new content' filePath='test.json' />,
    );
    const editors = container.querySelectorAll('.cm-editor');
    expect(editors.length).toBe(2);
  });

  it('both panes are read-only', () => {
    const { container } = render(
      <CM6DiffViewer oldContent='original' newContent='modified' filePath='data.yml' />,
    );
    const contentElements = container.querySelectorAll('.cm-content');
    for (const content of contentElements) {
      expect(content.getAttribute('contenteditable')).toBe('false');
    }
  });

  it('renders with identical content (no diff)', () => {
    const content = 'same\ncontent\nhere';
    const { container } = render(
      <CM6DiffViewer oldContent={content} newContent={content} filePath='unchanged.yml' />,
    );
    expect(container.querySelector('.cm-mergeView')).not.toBeNull();
  });

  it('handles empty content', () => {
    const { container } = render(
      <CM6DiffViewer oldContent='' newContent='new content added' filePath='new-file.yml' />,
    );
    expect(container.querySelector('.cm-mergeView')).not.toBeNull();
  });
});
