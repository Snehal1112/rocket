import { render, screen } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';

const read = (name: string) => readFileSync(resolve(process.cwd(), `src/components/ui/${name}.tsx`), 'utf8');
const OVERLAY_FILES = ['popover', 'dropdown-menu', 'context-menu', 'select', 'dialog', 'alert-dialog', 'sheet'];

// Scrims keep bg-background/20 and bg-black/80, so only a bare surface class is forbidden.
const OWN_SURFACE = /bg-dropdown-bg\/50|backdrop-blur-sm|shadow-\[|shadow-(md|lg)|bg-card|bg-background(?!\/20)/;

describe.each(OVERLAY_FILES)('%s', (name) => {
  it('uses the overlay level and no surface or shadow of its own', () => {
    const src = read(name);
    expect(src).toContain('elevation-overlay');
    expect(src).not.toMatch(OWN_SURFACE);
  });
});

it('renders dropdown content on the overlay level', async () => {
  render(
    <DropdownMenu defaultOpen>
      <DropdownMenuTrigger>Open</DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuItem>Item</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>,
  );
  expect(await screen.findByRole('menu')).toHaveClass('elevation-overlay');
});

it('aliases --dropdown-bg to the overlay token in dark', () => {
  const css = readFileSync(resolve(process.cwd(), 'src/globals.css'), 'utf8');
  expect(css).toMatch(/--dropdown-bg:\s*var\(--elevation-overlay-bg\)/);
});

it('renders a dialog on the overlay level and leaves its scrim alone', async () => {
  render(
    <Dialog defaultOpen>
      <DialogContent>
        <DialogTitle>Title</DialogTitle>
        <DialogDescription>Body</DialogDescription>
      </DialogContent>
    </Dialog>,
  );
  expect(await screen.findByRole('dialog')).toHaveClass('elevation-overlay');
  expect(read('dialog')).toContain('bg-black/80');
});
