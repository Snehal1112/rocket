// Placeholder — replaced with the real Request node UI in Task 2. Renders
// only the label, since FlowCanvas's node-data wiring depends on it
// appearing.
export function RequestNode({ data }: { data?: { kind?: { label?: string } } }) {
  return <div>{data?.kind?.label}</div>;
}
