import { useEffect, useRef, useState } from 'react';
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { type AnnounceRunState, type AnnounceSnapshot, diffAnnouncements } from './flowAnnounce';

// Turns status changes into the text of two live regions. The first render is
// the baseline, so opening a tab never announces an old run.
export function useFlowRunAnnouncer(
  nodes: FlowNode[],
  nodeStatus: Record<string, FlowNodeStatus>,
  nodeDetail: Record<string, FlowNodeDetail> | undefined,
  runState: AnnounceRunState,
): { polite: string; alert: string } {
  const previous = useRef<AnnounceSnapshot>({ runState, status: nodeStatus });
  const [messages, setMessages] = useState({ polite: '', alert: '' });

  useEffect(() => {
    const next: AnnounceSnapshot = { runState, status: nodeStatus };
    const out = diffAnnouncements(previous.current, next, nodes, nodeDetail);
    previous.current = next;
    if (out.polite.length === 0 && out.alerts.length === 0 && !out.runStarted) return;
    setMessages((current) => ({
      polite: out.polite.length > 0 ? out.polite.join(' ') : current.polite,
      alert: out.alerts.length > 0 ? out.alerts.join(' ') : out.runStarted ? '' : current.alert,
    }));
  }, [nodes, nodeStatus, nodeDetail, runState]);

  return messages;
}
