import { describe, expect, it } from 'vitest';
import { captureWorkspace } from '@/lib/workspace-guard';
import { useWorkspaceStore } from '@/stores/workspace-store';

describe('captureWorkspace', () => {
  it('is true until another workspace becomes active', () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'A' });
    const inA = captureWorkspace();
    expect(inA()).toBe(true);
    useWorkspaceStore.setState({ activeWorkspaceId: 'B' });
    expect(inA()).toBe(false);
  });
});
