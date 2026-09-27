// Placeholder — replaced with the real Input node UI in Task 3. Renders
// only the label, since FlowCanvas's node-data wiring depends on it
// appearing.
export function InputNode({ data }: { data?: { kind?: { label?: string } } }) {
  return <div>{data?.kind?.label}</div>;
}
