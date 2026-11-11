import { cleanup, render, screen } from '@testing-library/react';
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
  // The backend sends the fields of an event kind as snake_case, as the Rust enum has no rename.
  it('describes a new pairing', () => {
    show({ kind: 'network_device_paired', device_id: FULL_ID });
    expect(screen.getByText('Paired with device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a pairing the user removed', () => {
    show({ kind: 'network_device_unpaired', device_id: FULL_ID, by_peer: false });
    expect(screen.getByText('Removed the pairing with device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a pairing the other device removed', () => {
    show({ kind: 'network_device_unpaired', device_id: FULL_ID, by_peer: true });
    expect(screen.getByText('Device K7QF-2M9X removed the pairing')).toBeInTheDocument();
  });

  it('describes a block', () => {
    show({ kind: 'network_device_blocked', device_id: FULL_ID });
    expect(screen.getByText('Blocked device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a collection that was sent', () => {
    show({ kind: 'network_collection_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'completed' });
    expect(screen.getByText('Sent collection Orders to device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a collection that was received', () => {
    show({ kind: 'network_collection_received', device_id: FULL_ID, collection: 'Orders', outcome: 'completed' });
    expect(screen.getByText('Received collection Orders from device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a collection that was sent with a short id', () => {
    show({ kind: 'network_collection_sent', device_id: 'K7QF-2M9X', collection: 'Orders', outcome: 'completed' });
    expect(screen.getByText('Sent collection Orders to device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a collection that was received with a short id', () => {
    show({ kind: 'network_collection_received', device_id: 'K7QF-2M9X', collection: 'Orders', outcome: 'completed' });
    expect(screen.getByText('Received collection Orders from device K7QF-2M9X')).toBeInTheDocument();
  });

  it('shows a collection name with a double quote and unicode as given', () => {
    show({
      kind: 'network_collection_sent',
      device_id: FULL_ID,
      collection: 'Orders "v2" café',
      outcome: 'completed',
    });
    expect(screen.getByText('Sent collection Orders "v2" café to device K7QF-2M9X')).toBeInTheDocument();
  });

  it('shows the wire outcome of a send that did not complete', () => {
    for (const outcome of ['unconfirmed', 'failed', 'cancelled', 'damaged', 'rejected']) {
      show({ kind: 'network_collection_sent', device_id: FULL_ID, collection: 'Orders', outcome });
      expect(
        screen.getByText(`Sent collection Orders to device K7QF-2M9X (${outcome})`),
      ).toBeInTheDocument();
      cleanup();
    }
  });

  it('shows a rejected and a damaged receive', () => {
    show({ kind: 'network_collection_received', device_id: FULL_ID, collection: 'Orders', outcome: 'rejected' });
    expect(
      screen.getByText('Received collection Orders from device K7QF-2M9X (rejected)'),
    ).toBeInTheDocument();
    cleanup();
    show({ kind: 'network_collection_received', device_id: FULL_ID, collection: 'Orders', outcome: 'damaged' });
    expect(
      screen.getByText('Received collection Orders from device K7QF-2M9X (damaged)'),
    ).toBeInTheDocument();
  });

  it('describes a sync that was sent, with the short id and no word for success', () => {
    show({ kind: 'network_sync_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'applied' });
    expect(screen.getByText('Sent sync changes for Orders to device K7QF-2M9X')).toBeInTheDocument();
  });

  it('describes a sync that was received, with the short id and no word for success', () => {
    show({
      kind: 'network_sync_received',
      device_id: FULL_ID,
      collection: 'Orders',
      outcome: 'applied',
    });
    expect(
      screen.getByText('Received sync changes for Orders from device K7QF-2M9X'),
    ).toBeInTheDocument();
  });

  it('shows the outcome of a sync that did not complete', () => {
    show({ kind: 'network_sync_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'failed' });
    expect(
      screen.getByText('Sent sync changes for Orders to device K7QF-2M9X (failed)'),
    ).toBeInTheDocument();
    cleanup();
    show({
      kind: 'network_sync_received',
      device_id: FULL_ID,
      collection: 'Orders',
      outcome: 'unconfirmed',
    });
    expect(
      screen.getByText('Received sync changes for Orders from device K7QF-2M9X (unconfirmed)'),
    ).toBeInTheDocument();
  });

  it('describes a removed link', () => {
    show({ kind: 'network_link_removed', device_id: FULL_ID, collection: 'Orders' });
    expect(screen.getByText('Unlinked collection Orders from device K7QF-2M9X')).toBeInTheDocument();
  });

  it('shows a collection name with a double quote as given', () => {
    show({ kind: 'network_link_removed', device_id: FULL_ID, collection: 'Orders "v2"' });
    expect(
      screen.getByText('Unlinked collection Orders "v2" from device K7QF-2M9X'),
    ).toBeInTheDocument();
  });

  it('does not show the full device id for the sync kinds', () => {
    show({ kind: 'network_sync_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'applied' });
    expect(screen.queryByText(new RegExp(FULL_ID))).toBeNull();
  });
});

describe('AuditEventRow unknown kinds', () => {
  it('names an event kind from a newer version', () => {
    show({ kind: 'from_the_future' } as unknown as AuditEventKind);
    expect(screen.getByText('Event from a newer version (from_the_future)')).toBeInTheDocument();
  });
});
