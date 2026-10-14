import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@/components/ui/dialog';

describe('DialogBody', () => {
  it('is a scrolling flex child that can shrink', () => {
    render(<DialogBody data-testid='body'>content</DialogBody>);
    const body = screen.getByTestId('body');
    expect(body).toHaveAttribute('data-slot', 'dialog-body');
    expect(body).toHaveClass('overflow-auto', 'flex-1', 'min-h-0');
  });

  it('merges a custom className and forwards props', () => {
    render(
      <DialogBody data-testid='body' className='space-y-4' id='b1'>
        content
      </DialogBody>,
    );
    const body = screen.getByTestId('body');
    expect(body).toHaveClass('space-y-4', 'overflow-auto');
    expect(body).toHaveAttribute('id', 'b1');
  });

  it('sits inside a dialog that keeps max-w-lg and a footer', () => {
    render(
      <Dialog open>
        <DialogContent className='flex max-h-[85vh] flex-col'>
          <DialogTitle>Title</DialogTitle>
          <DialogDescription>Description</DialogDescription>
          <DialogBody data-testid='body'>content</DialogBody>
          <DialogFooter data-testid='footer'>actions</DialogFooter>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByRole('dialog')).toHaveClass('max-w-lg', 'flex', 'flex-col');
    expect(screen.getByTestId('body')).toBeInTheDocument();
    expect(screen.getByTestId('footer')).toHaveClass('sm:justify-end');
  });
});
