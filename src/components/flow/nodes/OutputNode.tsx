// Placeholder — replaced with the real Output node UI in Task 3. Renders
// only the label, since FlowCanvas's node-data wiring depends on it
// appearing.
export function OutputNode({ data }: { data?: { kind?: { label?: string } } }) {
  return <div>{data?.kind?.label}</div>;
}
