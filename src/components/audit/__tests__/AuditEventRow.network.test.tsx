import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { AuditEventRow } from '@/components/audit/AuditEventRow';
import type { AuditEventKind, SecurityAuditEvent } from '@/lib/tauri-api';

const FULL_ID = 'K7QF2M9XPYE7HWE7ABCDEFGHIJKLMNOPQRSTUVWXYZ23';

const makeEvent = (event: AuditEventKind): SecurityAuditEvent => ({
  id: 'e1',
  occurredAt: '2026-11-06T10:00:00Z',
  actor: 'user',
  workspaceId: null,
  event,
  controls: [],
  prevHash: '',
  hash: 'abcdef0123456789',
});

const show = (event: AuditEventKind) =>
  render(
    <table>
      <tbody>
        <AuditEventRow event={makeEvent(event)} />
      </tbody>
    </table>,
  );

describe('AuditEventRow network kinds', () => {
  it('describes a new pairing', () => {
    show({ kind: 'network_device_paired', deviceId: FULL_ID });
    expect(screen.getByText('Paired with device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a pairing the user removed', () => {
    show({ kind: 'network_device_unpaired', deviceId: FULL_ID, byPeer: false });
    expect(screen.getByText('Removed the pairing with device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a pairing the other device removed', () => {
    show({ kind: 'network_device_unpaired', deviceId: FULL_ID, byPeer: true });
    expect(screen.getByText('Device K7QF-2M9X removed the pairing')).toBeInTheDocument();
  });

  it('describes a block', () => {
    show({ kind: 'network_device_blocked', deviceId: FULL_ID });
    expect(screen.getByText('Blocked device K7QF-2M9X')).toBeInTheDocument();
  });
});
