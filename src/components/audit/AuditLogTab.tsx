import { AlertTriangle, Download, ShieldCheck, Sliders } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { AuditEventRow } from '@/components/audit/AuditEventRow';
import { ComplianceProfileDialog } from '@/components/audit/ComplianceProfileDialog';
import { ExportEvidenceDialog } from '@/components/audit/ExportEvidenceDialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useAuditStore } from '@/stores/audit-store';

export function AuditLogTab() {
  const events = useAuditStore((s) => s.events);
  const profile = useAuditStore((s) => s.profile);
  const loading = useAuditStore((s) => s.loading);
  const error = useAuditStore((s) => s.error);
  const loadEvents = useAuditStore((s) => s.loadEvents);
  const loadProfile = useAuditStore((s) => s.loadProfile);

  const [profileOpen, setProfileOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [filter, setFilter] = useState('');

  useEffect(() => {
    // Only fetch if the store is empty — avoid re-fetching on every tab switch.
    if (events.length === 0) void loadEvents();
    if (!profile) void loadProfile();
  }, [loadEvents, loadProfile, events.length, profile]);

  const filtered = useMemo(() => {
    if (!filter.trim()) return events;
    const q = filter.toLowerCase();
    return events.filter((e) => {
      const blob = `${e.actor} ${e.event.kind} ${JSON.stringify(e.event)} ${e.controls
        .map((c) => `${c.framework} ${c.code}`)
        .join(' ')}`.toLowerCase();
      return blob.includes(q);
    });
  }, [events, filter]);

  const activeCount = profile?.activeFrameworks.length ?? 0;

  return (
    <div className='h-full flex flex-col bg-card'>
      {/* Header */}
      <div className='shrink-0 border-b border-border/70 px-6 py-3 flex items-center justify-between'>
        <div className='flex items-center gap-2'>
          <ShieldCheck className='h-4 w-4 text-muted-foreground' />
          <h1 className='text-sm font-semibold'>Audit Log</h1>
          <span className='text-xs text-muted-foreground'>
            · {events.length} event{events.length === 1 ? '' : 's'}
            {activeCount > 0 && ` · ${activeCount} framework${activeCount === 1 ? '' : 's'} active`}
          </span>
        </div>
        <div className='flex items-center gap-2'>
          <Button
            variant='outline'
            size='sm'
            className='h-7 text-xs'
            onClick={() => setProfileOpen(true)}
          >
            <Sliders className='h-3 w-3 mr-1.5' />
            Profile
          </Button>
          <Button
            variant='outline'
            size='sm'
            className='h-7 text-xs'
            onClick={() => setExportOpen(true)}
          >
            <Download className='h-3 w-3 mr-1.5' />
            Export evidence
          </Button>
        </div>
      </div>

      {/* Filter */}
      <div className='shrink-0 px-6 py-2 border-b border-border/50'>
        <Input
          placeholder='Filter by actor, kind, control ID...'
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className='h-7 text-xs'
          aria-label='Filter audit events'
        />
      </div>

      {/* Table */}
      <ScrollArea className='flex-1'>
        {error && (
          <div className='flex items-center gap-2 px-6 py-3 text-xs text-destructive'>
            <AlertTriangle className='h-3.5 w-3.5' />
            {error}
          </div>
        )}
        {loading && events.length === 0 ? (
          <div className='px-6 py-8 text-xs text-muted-foreground'>Loading audit log…</div>
        ) : filtered.length === 0 ? (
          <div className='px-6 py-16 text-center'>
            <ShieldCheck className='h-10 w-10 mx-auto text-muted-foreground/40' />
            <p className='mt-3 text-sm font-medium text-foreground'>No audit events</p>
            <p className='mt-1 text-xs text-muted-foreground'>
              Sensitive operations will appear here once they occur.
            </p>
          </div>
        ) : (
          <table className='w-full text-left'>
            <thead className='text-[10px] uppercase tracking-wider text-muted-foreground border-b border-border/60 sticky top-0 bg-card'>
              <tr>
                <th scope='col' className='py-2 px-3 font-medium'>
                  When
                </th>
                <th scope='col' className='py-2 px-3 font-medium'>
                  Event
                </th>
                <th scope='col' className='py-2 px-3 font-medium'>
                  Controls
                </th>
                <th scope='col' className='py-2 px-3 font-medium'>
                  Hash
                </th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((ev) => (
                <AuditEventRow key={ev.id} event={ev} />
              ))}
            </tbody>
          </table>
        )}
      </ScrollArea>

      <ComplianceProfileDialog open={profileOpen} onOpenChange={setProfileOpen} />
      <ExportEvidenceDialog open={exportOpen} onOpenChange={setExportOpen} />
    </div>
  );
}
