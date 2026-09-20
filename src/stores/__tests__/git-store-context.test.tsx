import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider, useGitStore, useGitStoreApi } from '@/stores/git-store-context';

function Probe() {
  const isRepo = useGitStore((s) => s.isRepo);
  const api = useGitStoreApi();
  return (
    <div>
      <span data-testid='is-repo'>{String(isRepo)}</span>
      <button type='button' onClick={() => api.setState({ isRepo: true })}>
        flip
      </button>
    </div>
  );
}

describe('GitStoreProvider / useGitStore', () => {
  it('reads reactive state scoped to the provided store instance', () => {
    const store = createGitStore();
    render(
      <GitStoreProvider store={store}>
        <Probe />
      </GitStoreProvider>,
    );
    expect(screen.getByTestId('is-repo').textContent).toBe('false');
  });

  it('two providers with different store instances are fully isolated', () => {
    const storeA = createGitStore();
    const storeB = createGitStore();
    storeA.setState({ isRepo: true });

    render(
      <>
        <GitStoreProvider store={storeA}>
          <Probe />
        </GitStoreProvider>
        <GitStoreProvider store={storeB}>
          <Probe />
        </GitStoreProvider>
      </>,
    );
    const [first, second] = screen.getAllByTestId('is-repo');
    expect(first.textContent).toBe('true');
    expect(second.textContent).toBe('false');
  });

  it('throws a clear error when used outside a provider', () => {
    const OutsideProbe = () => {
      useGitStore((s) => s.isRepo);
      return null;
    };
    expect(() => render(<OutsideProbe />)).toThrow(
      /useGitStore must be used within a GitStoreProvider/,
    );
  });
});
