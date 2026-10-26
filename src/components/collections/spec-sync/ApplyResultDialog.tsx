import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { OpenApiApplyReport } from '@/lib/tauri-api';

interface ApplyResultDialogProps {
  /** The report to show, or null while the dialog is closed. */
  report: OpenApiApplyReport | null;
  /** How many variable changes were part of the apply. */
  variableCount: number;
  onClose: () => void;
  onCheckAgain: () => void;
  checking: boolean;
}

function KeyList({ keys }: { keys: string[] }) {
  return (
    <ul className='ml-4 list-disc break-all font-mono text-xs text-muted-foreground'>
      {keys.map((k) => (
        <li key={k}>{k}</li>
      ))}
    </ul>
  );
}

/** What an apply did, in plain words. Only the applied list is claimed as done. */
export function ApplyResultDialog({
  report,
  variableCount,
  onClose,
  onCheckAgain,
  checking,
}: ApplyResultDialogProps) {
  return (
    <Dialog open={report !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Apply finished</DialogTitle>
          <DialogDescription>Here is what happened to your selection.</DialogDescription>
        </DialogHeader>
        {report && (
          <DialogBody>
            <div className='flex flex-col gap-3 text-sm'>
              {report.applied.length === 0 && variableCount === 0 && <p>Nothing was applied.</p>}
              {report.applied.length > 0 && (
                <div className='flex flex-col gap-1'>
                  <p>
                    {`${report.applied.length} applied: ${
                      report.applied.filter((a) => a.kind === 'created').length
                    } created, ${report.applied.filter((a) => a.kind === 'updated').length} updated.`}
                  </p>
                  <KeyList keys={report.applied.map((a) => a.requestPath)} />
                </div>
              )}
              {report.stale.length > 0 && (
                <div className='flex flex-col gap-1'>
                  <p>
                    {`${report.stale.length} not applied because the spec changed since the list was made. Check again to see the latest.`}
                  </p>
                  <KeyList keys={report.stale} />
                </div>
              )}
              {variableCount > 0 && (
                <p>
                  {`${variableCount} variable ${variableCount === 1 ? 'change was' : 'changes were'} checked. A value you changed yourself was kept.`}
                </p>
              )}
              {report.notRewritable.length > 0 && (
                <div className='flex flex-col gap-1'>
                  <p>
                    {report.notRewritable.length === 1
                      ? '1 left as it is because Rocket cannot rewrite its file safely.'
                      : `${report.notRewritable.length} left as they are because Rocket cannot rewrite their files safely.`}
                  </p>
                  <KeyList keys={report.notRewritable} />
                </div>
              )}
              {report.deleted.length > 0 && (
                <div className='flex flex-col gap-1'>
                  <p>
                    {`${report.deleted.length} deleted because the spec removed ${report.deleted.length === 1 ? 'it' : 'them'}.`}
                  </p>
                  <KeyList keys={report.deleted} />
                </div>
              )}
              {report.detached.length > 0 && (
                <div className='flex flex-col gap-1'>
                  <p>
                    {`${report.detached.length} removed from the spec but kept in your collection, no longer tracked.`}
                  </p>
                  <KeyList keys={report.detached} />
                </div>
              )}
              {report.skipped > 0 && <p>{`${report.skipped} skipped.`}</p>}
              {report.remaining > 0 && (
                <p>{`${report.remaining} ${report.remaining === 1 ? 'change' : 'changes'} still waiting.`}</p>
              )}
            </div>
          </DialogBody>
        )}
        <DialogFooter>
          <Button variant='outline' onClick={onCheckAgain} disabled={checking}>
            Check again
          </Button>
          <Button onClick={onClose}>Close</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
