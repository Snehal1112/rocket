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
              {report.applied.length === 0 ? (
                <p>Nothing was applied.</p>
              ) : (
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
              {report.notRewritable.length > 0 && (
                <div className='flex flex-col gap-1'>
                  <p>
                    {`${report.notRewritable.length} left as it is because Rocket cannot rewrite its file safely.`}
                  </p>
                  <KeyList keys={report.notRewritable} />
                </div>
              )}
              {report.deleted.length > 0 && (
                <p>{`${report.deleted.length} deleted because the spec removed it.`}</p>
              )}
              {report.detached.length > 0 && (
                <p>
                  {`${report.detached.length} removed from the spec but kept in your collection, no longer tracked.`}
                </p>
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
