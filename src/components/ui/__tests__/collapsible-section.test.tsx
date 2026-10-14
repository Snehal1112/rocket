import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { CollapsibleSection } from '@/components/ui/collapsible-section';

describe('CollapsibleSection', () => {
  it('is closed by default and shows the title and summary', () => {
    render(
      <CollapsibleSection title='Advanced' summary='Idle 300s, 2 variables'>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const trigger = screen.getByRole('button', { name: /Advanced/ });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByText('Idle 300s, 2 variables')).toBeInTheDocument();
    expect(screen.queryByText('Inner content')).toBeNull();
  });

  it('opens and closes when the trigger is clicked', async () => {
    render(
      <CollapsibleSection title='Advanced'>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const trigger = screen.getByRole('button', { name: /Advanced/ });
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Inner content')).toBeInTheDocument();
    await userEvent.click(trigger);
    expect(screen.queryByText('Inner content')).toBeNull();
  });

  it('opens with the keyboard', async () => {
    render(
      <CollapsibleSection title='Advanced'>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    screen.getByRole('button', { name: /Advanced/ }).focus();
    await userEvent.keyboard('{Enter}');
    expect(screen.getByText('Inner content')).toBeInTheDocument();
  });

  it('honours defaultOpen', () => {
    render(
      <CollapsibleSection title='Advanced' defaultOpen>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    expect(screen.getByRole('button', { name: /Advanced/ })).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    expect(screen.getByText('Inner content')).toBeInTheDocument();
  });

  it('follows the open prop when controlled and reports toggles', async () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(
      <CollapsibleSection title='Advanced' open={false} onOpenChange={onOpenChange}>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const trigger = screen.getByRole('button', { name: /Advanced/ });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(trigger);
    expect(onOpenChange).toHaveBeenLastCalledWith(true);
    // The parent did not change the prop, so the section stays closed.
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText('Inner content')).toBeNull();

    rerender(
      <CollapsibleSection title='Advanced' open onOpenChange={onOpenChange}>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Inner content')).toBeInTheDocument();
    await userEvent.click(trigger);
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
  });

  it('still honours defaultOpen when uncontrolled and reports toggles', async () => {
    const onOpenChange = vi.fn();
    render(
      <CollapsibleSection title='Advanced' defaultOpen onOpenChange={onOpenChange}>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const trigger = screen.getByRole('button', { name: /Advanced/ });
    expect(screen.getByText('Inner content')).toBeInTheDocument();
    await userEvent.click(trigger);
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(screen.queryByText('Inner content')).toBeNull();
  });

  it('does not mount the children while closed', () => {
    let mounts = 0;
    function Probe() {
      mounts += 1;
      return <p>Probe content</p>;
    }
    const { rerender } = render(
      <CollapsibleSection title='Advanced'>
        <Probe />
      </CollapsibleSection>,
    );
    expect(mounts).toBe(0);
    expect(screen.queryByText('Probe content')).toBeNull();
    rerender(
      <CollapsibleSection title='Advanced' open>
        <Probe />
      </CollapsibleSection>,
    );
    expect(mounts).toBeGreaterThan(0);
    expect(screen.getByText('Probe content')).toBeInTheDocument();
  });

  it('keeps a very long summary on one line with an ellipsis', () => {
    const long = 'z'.repeat(300);
    render(
      <CollapsibleSection title='Advanced' summary={long}>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    expect(screen.getByText(long)).toHaveClass('truncate', 'min-w-0');
  });

  it('disables the chevron transition for reduced motion', () => {
    render(
      <CollapsibleSection title='Advanced'>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const chevron = screen.getByRole('button', { name: /Advanced/ }).querySelector('svg');
    expect(chevron).toHaveClass('transition-transform', 'motion-reduce:transition-none');
    expect(chevron).toHaveAttribute('aria-hidden', 'true');
  });
});
