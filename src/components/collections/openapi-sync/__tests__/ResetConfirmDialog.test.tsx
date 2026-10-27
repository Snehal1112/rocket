import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ResetConfirmDialog } from '../ResetConfirmDialog';

const LONG_URL =
  '{{baseUrl}}/user/installations/:installation_id/repositories/:repository_id?as=as';

describe('ResetConfirmDialog', () => {
  // jsdom has no layout, so the rule that keeps a long URL inside the card is checked by class.
  it('lets_a_long_url_wrap_inside_the_card', () => {
    render(
      <ResetConfirmDialog
        open
        onOpenChange={vi.fn()}
        title='Reset to the spec?'
        body={`Reset PUT ${LONG_URL} to the spec?`}
        confirmLabel='Reset'
        onConfirm={vi.fn()}
      />,
    );
    const body = screen.getByText(`Reset PUT ${LONG_URL} to the spec?`);
    expect(body.className).toContain('[overflow-wrap:anywhere]');
    expect(body.parentElement?.className).toContain('min-w-0');
  });
});
