import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import type { AnnounceRunState } from './flowAnnounce';
import { useFlowRunAnnouncer } from './useFlowRunAnnouncer';

interface FlowRunAnnouncerProps {
  nodes: FlowNode[];
  nodeStatus: Record<string, FlowNodeStatus>;
  nodeDetail?: Record<string, FlowNodeDetail>;
  runState: AnnounceRunState;
}

// Two screen-reader-only regions. They are always rendered, because a live
// region is only announced when it already exists before its text changes.
export function FlowRunAnnouncer({
  nodes,
  nodeStatus,
  nodeDetail,
  runState,
}: FlowRunAnnouncerProps) {
  const { polite, alert } = useFlowRunAnnouncer(nodes, nodeStatus, nodeDetail, runState);
  return (
    <>
      <div
        data-testid='flow-announcer-status'
        role='status'
        aria-live='polite'
        aria-atomic='true'
        className='sr-only'
      >
        {polite}
      </div>
      <div data-testid='flow-announcer-alert' role='alert' className='sr-only'>
        {alert}
      </div>
    </>
  );
}
