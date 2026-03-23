import type { PaneNode } from '@/types/pane-types';
import { SplitPane } from './SplitPane';
import { EditorGroup } from './EditorGroup';

// Recursive entry point that renders the correct component based on node type.
export function PaneRenderer({ node }: { node: PaneNode }) {
  if (node.type === 'split') return <SplitPane node={node} />;
  return <EditorGroup node={node} />;
}
