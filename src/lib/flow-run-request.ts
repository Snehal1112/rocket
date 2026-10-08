import type { FlowPartialMode } from '@/lib/tauri-api';

/** Window event that asks a flow tab's toolbar to start a run. */
export const FLOW_RUN_EVENT = 'rocket:flow-run';

/** The part of the flow to re-run. The toolbar adds the tab's last run as the base. */
export interface PartialRunRequest {
  startNodeId: string;
  mode: FlowPartialMode;
}

export interface FlowRunRequestDetail {
  tabId: string;
  /** Absent for a full run. */
  partial?: PartialRunRequest;
}

/** Asks the toolbar of `detail.tabId` to run, so every run shares one lifecycle. */
export function requestFlowRun(detail: FlowRunRequestDetail): void {
  window.dispatchEvent(new CustomEvent<FlowRunRequestDetail>(FLOW_RUN_EVENT, { detail }));
}
