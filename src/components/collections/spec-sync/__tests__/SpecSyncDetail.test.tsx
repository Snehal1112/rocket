import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SpecSyncDetail } from '../SpecSyncDetail';
import { makePlan } from './fixtures';

describe('SpecSyncDetail', () => {
  it('shows each changed field with baseline, yours, spec and the outcome', () => {
    render(<SpecSyncDetail plan={makePlan()} selectedId='changed:POST /pets' />);
    expect(screen.getByText('Header Accept')).toBeInTheDocument();
    expect(screen.getByText('Body $.a.b')).toBeInTheDocument();
    expect(screen.getByText('Spec wins')).toBeInTheDocument();
    expect(screen.getByText('Yours kept')).toBeInTheDocument();
    expect(screen.getByText('Conflict')).toBeInTheDocument();
    expect(screen.getAllByText('Baseline')).toHaveLength(3);
    expect(screen.getByText('a/c')).toBeInTheDocument();
    expect(screen.getByText('u2')).toBeInTheDocument();
  });

  it('shows the request summary and folder of a new item', () => {
    render(<SpecSyncDetail plan={makePlan()} selectedId='new:GET /pets' />);
    expect(screen.getByText('List pets')).toBeInTheDocument();
    expect(screen.getByText('Folder: pets / read')).toBeInTheDocument();
    expect(screen.getByText('Possibly renamed from GET /animals')).toBeInTheDocument();
  });

  it('says a removed item of unknown edit state is kept', () => {
    render(<SpecSyncDetail plan={makePlan()} selectedId='removed:DELETE /pets' />);
    expect(screen.getByText('Treated as edited, will be kept')).toBeInTheDocument();
  });

  it('shows the cautious warning and no action controls', () => {
    render(<SpecSyncDetail plan={makePlan({ cautious: true })} selectedId='new:GET /pets' />);
    expect(screen.getByRole('alert')).toHaveTextContent(/plays safe/);
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.queryByRole('checkbox')).toBeNull();
  });

  it('renders only the selected item', () => {
    render(<SpecSyncDetail plan={makePlan()} selectedId='new:GET /pets' />);
    expect(screen.queryByText('Add pet')).toBeNull();
    expect(screen.queryByText('Delete pet')).toBeNull();
  });
});
