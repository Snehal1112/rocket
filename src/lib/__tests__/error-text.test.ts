import { beforeEach, describe, expect, it, vi } from 'vitest';
import { errorText, toastError } from '@/lib/error-text';

const mocks = vi.hoisted(() => ({ toast: { error: vi.fn() } }));

vi.mock('sonner', () => ({ toast: mocks.toast }));

describe('errorText', () => {
  it('drops the backend error type prefix', () => {
    expect(errorText(new Error('Invalid input: Port is taken'))).toBe('Port is taken');
    expect(errorText('Internal error: boom')).toBe('boom');
  });

  it('keeps other messages as they are', () => {
    expect(errorText('Could not reach 10.0.0.5')).toBe('Could not reach 10.0.0.5');
  });

  it('trims the text', () => {
    expect(errorText('Invalid input:   spaced out  ')).toBe('spaced out');
  });

  it('falls back to a generic line when there is no text', () => {
    expect(errorText(42)).toBe('Something went wrong. Try again.');
    expect(errorText('')).toBe('Something went wrong. Try again.');
    expect(errorText(null)).toBe('Something went wrong. Try again.');
  });

  it('uses the given fallback instead of the generic line', () => {
    expect(errorText(42, 'Could not load.')).toBe('Could not load.');
    expect(errorText('Invalid input: ', 'Could not load.')).toBe('Could not load.');
  });
});

describe('toastError', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows the cleaned text as an error toast', () => {
    toastError('Internal error: disk full');
    expect(mocks.toast.error).toHaveBeenCalledWith('disk full');
  });

  it('puts the prefix in front of the text', () => {
    toastError(new Error('Invalid input: Port is taken'), 'Could not add the address.');
    expect(mocks.toast.error).toHaveBeenCalledWith('Could not add the address. Port is taken');
  });

  it('shows the generic line for an unknown error', () => {
    toastError(undefined);
    expect(mocks.toast.error).toHaveBeenCalledWith('Something went wrong. Try again.');
  });
});
