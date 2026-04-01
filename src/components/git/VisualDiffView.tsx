import { cn } from "@/lib/utils";
import { parseRequestDiff } from "@/lib/parse-request-diff";
import type { RowChange } from "@/types/visual-diff-types";

interface VisualDiffViewProps {
  oldContent: string | undefined;
  newContent: string | undefined;
}

// Renders a single labeled field change. Shows old/new on separate rows when changed.
function DiffField({
  label,
  oldValue,
  newValue,
  changed,
}: {
  label: string;
  oldValue: string | undefined;
  newValue: string | undefined;
  changed: boolean;
}) {
  if (!changed) {
    return (
      <tr>
        <td className="py-1 pr-4 text-muted-foreground w-32 align-top">
          {label}
        </td>
        <td className="py-1 font-mono text-xs" colSpan={2}>
          {newValue ?? oldValue ?? "—"}
        </td>
      </tr>
    );
  }
  return (
    <>
      {oldValue !== undefined && (
        <tr className="bg-red-50 dark:bg-red-950/20">
          <td className="py-1 pr-4 text-muted-foreground w-32 align-top">
            {label}
          </td>
          <td className="py-1 pr-2 text-xs text-red-500 dark:text-red-400 w-8 align-top">
            old
          </td>
          <td className="py-1 font-mono text-xs text-red-700 dark:text-red-300 line-through break-all">
            {oldValue}
          </td>
        </tr>
      )}
      {newValue !== undefined && (
        <tr className="bg-green-50 dark:bg-green-950/20">
          <td className="py-1 pr-4 text-muted-foreground w-32 align-top">
            {oldValue !== undefined ? "" : label}
          </td>
          <td className="py-1 pr-2 text-xs text-green-500 dark:text-green-400 w-8 align-top">
            new
          </td>
          <td className="py-1 font-mono text-xs text-green-700 dark:text-green-300 break-all">
            {newValue}
          </td>
        </tr>
      )}
    </>
  );
}

const ROW_BG: Record<RowChange["status"], string> = {
  added: "bg-green-50 dark:bg-green-950/20",
  removed: "bg-red-50 dark:bg-red-950/20",
  modified: "bg-amber-50 dark:bg-amber-950/20",
  unchanged: "",
};

const ROW_BADGE: Record<RowChange["status"], string> = {
  added: "text-green-600 dark:text-green-400",
  removed: "text-red-600 dark:text-red-400",
  modified: "text-amber-600 dark:text-amber-400",
  unchanged: "text-muted-foreground",
};

const ROW_LABEL: Record<RowChange["status"], string> = {
  added: "A",
  removed: "D",
  modified: "M",
  unchanged: "",
};

// Renders a key/value list diff (headers, params) as a table with status indicators.
function KVTable({ title, rows }: { title: string; rows: RowChange[] }) {
  if (rows.length === 0) return null;
  return (
    <section>
      <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
        {title}
      </h3>
      <div className="border rounded-md overflow-hidden">
        <table className="w-full text-xs">
          <tbody>
            {rows.map((row) => (
              <tr
                key={row.key}
                className={cn("border-b last:border-0", ROW_BG[row.status])}
              >
                <td
                  className={cn(
                    "py-1.5 pl-2 pr-1 font-bold w-4",
                    ROW_BADGE[row.status],
                  )}
                >
                  {ROW_LABEL[row.status]}
                </td>
                <td className="py-1.5 px-2 font-mono text-muted-foreground w-1/3">
                  {row.key}
                </td>
                <td className="py-1.5 px-2 font-mono opacity-60 w-1/3 line-through">
                  {row.status === "removed" || row.status === "modified"
                    ? (row.oldRow?.value ?? "")
                    : ""}
                </td>
                <td className="py-1.5 px-2 font-mono">
                  {row.status === "added" || row.status === "modified"
                    ? (row.newRow?.value ?? "")
                    : row.status === "unchanged"
                      ? (row.newRow?.value ?? "")
                      : ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

// Renders API request file changes as a structured field-by-field comparison.
export function VisualDiffView({
  oldContent,
  newContent,
}: VisualDiffViewProps) {
  const diff = parseRequestDiff(oldContent, newContent);

  if (!diff) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        Visual diff is not available for this file type.
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-5 text-sm">
      {/* Request — method and URL. */}
      <section>
        <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
          Request
        </h3>
        <div className="border rounded-md overflow-hidden px-3 py-1">
          <table className="w-full text-xs">
            <tbody>
              <DiffField
                label="Method"
                oldValue={diff.method.oldValue}
                newValue={diff.method.newValue}
                changed={diff.method.changed}
              />
              <DiffField
                label="URL"
                oldValue={diff.url.oldValue}
                newValue={diff.url.newValue}
                changed={diff.url.changed}
              />
            </tbody>
          </table>
        </div>
      </section>

      <KVTable title="Headers" rows={diff.headers} />
      <KVTable title="Query Params" rows={diff.queryParams} />
      <KVTable title="Path Params" rows={diff.pathParams} />

      {/* Body — show only when body mode changed. */}
      {diff.body.changed && (
        <section>
          <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
            Body
          </h3>
          <div className="border rounded-md overflow-hidden px-3 py-1">
            <table className="w-full text-xs">
              <tbody>
                <DiffField
                  label="Mode"
                  oldValue={diff.body.oldValue?.mode}
                  newValue={diff.body.newValue?.mode}
                  changed={
                    diff.body.oldValue?.mode !== diff.body.newValue?.mode
                  }
                />
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* Auth — show only when auth type changed. */}
      {diff.auth.changed && (
        <section>
          <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
            Auth
          </h3>
          <div className="border rounded-md overflow-hidden px-3 py-1">
            <table className="w-full text-xs">
              <tbody>
                <DiffField
                  label={diff.auth.label}
                  oldValue={diff.auth.oldValue}
                  newValue={diff.auth.newValue}
                  changed={diff.auth.changed}
                />
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* Scripts — show only when pre/post scripts changed. */}
      {(diff.preRequestScript.changed || diff.postResponseScript.changed) && (
        <section>
          <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1.5">
            Scripts
          </h3>
          <div className="border rounded-md overflow-hidden px-3 py-1">
            <table className="w-full text-xs">
              <tbody>
                {diff.preRequestScript.changed && (
                  <DiffField
                    label={diff.preRequestScript.label}
                    oldValue={
                      diff.preRequestScript.oldValue !== undefined
                        ? `${diff.preRequestScript.oldValue.slice(0, 60)}${diff.preRequestScript.oldValue.length > 60 ? "…" : ""}`
                        : undefined
                    }
                    newValue={
                      diff.preRequestScript.newValue !== undefined
                        ? `${diff.preRequestScript.newValue.slice(0, 60)}${diff.preRequestScript.newValue.length > 60 ? "…" : ""}`
                        : undefined
                    }
                    changed={diff.preRequestScript.changed}
                  />
                )}
                {diff.postResponseScript.changed && (
                  <DiffField
                    label={diff.postResponseScript.label}
                    oldValue={
                      diff.postResponseScript.oldValue !== undefined
                        ? `${diff.postResponseScript.oldValue.slice(0, 60)}${diff.postResponseScript.oldValue.length > 60 ? "…" : ""}`
                        : undefined
                    }
                    newValue={
                      diff.postResponseScript.newValue !== undefined
                        ? `${diff.postResponseScript.newValue.slice(0, 60)}${diff.postResponseScript.newValue.length > 60 ? "…" : ""}`
                        : undefined
                    }
                    changed={diff.postResponseScript.changed}
                  />
                )}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {!diff.hasChanges && (
        <p className="text-center text-xs text-muted-foreground py-8">
          No changes detected.
        </p>
      )}
    </div>
  );
}
