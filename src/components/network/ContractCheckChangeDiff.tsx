import { DiffViewer } from '@/components/git/DiffViewer';
import type { ReviewedContractChange } from '@/lib/tauri-api';

// A missing side is a whole-value add or delete, which the viewer shows inline.
function diffStatus(change: ReviewedContractChange): string {
  if (change.oldValue === null) return 'added';
  if (change.newValue === null) return 'deleted';
  return 'modified';
}

/**
 * The before and after of one offered change in the read-only diff viewer. Values are shown as
 * they arrived, so a redacted value stays "[hidden]".
 */
export function ContractCheckChangeDiff({
  checkId,
  change,
}: {
  checkId: string;
  change: ReviewedContractChange;
}) {
  return (
    <div className='h-32 overflow-hidden rounded-md border'>
      {/* The key is needed because DiffViewer copies its prop into state once. */}
      <DiffViewer
        key={`${checkId}:${change.index}`}
        hideStageToggle
        diffState={{
          filePath: `${change.method} ${change.pathTemplate} ${change.field}`,
          repositoryId: '',
          repositoryLabel: 'Contract check',
          oldContent: change.oldValue ?? '',
          newContent: change.newValue ?? '',
          status: diffStatus(change),
          isStaged: false,
        }}
      />
    </div>
  );
}
