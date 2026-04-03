import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Textarea } from "@/components/ui/textarea";

interface MarkdownEditorProps {
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
}

export function MarkdownEditor({
  value,
  onChange,
  onBlur,
}: MarkdownEditorProps) {
  const [mode, setMode] = useState<"edit" | "preview">("preview");

  return (
    <div className="space-y-2">
      <div className="flex gap-1">
        <button
          type="button"
          className={`px-2 py-0.5 text-xs rounded ${
            mode === "edit"
              ? "bg-muted text-foreground"
              : "text-muted-foreground hover:text-foreground"
          }`}
          onClick={() => setMode("edit")}
        >
          Edit
        </button>
        <button
          type="button"
          className={`px-2 py-0.5 text-xs rounded ${
            mode === "preview"
              ? "bg-muted text-foreground"
              : "text-muted-foreground hover:text-foreground"
          }`}
          onClick={() => setMode("preview")}
        >
          Preview
        </button>
      </div>
      {mode === "edit" ? (
        <Textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={onBlur}
          placeholder="Write markdown here..."
          className="min-h-[200px] font-mono text-sm"
        />
      ) : (
        <div className="prose prose-sm dark:prose-invert max-w-none min-h-[200px] p-3 border rounded-md">
          {value ? (
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{value}</ReactMarkdown>
          ) : (
            <p className="text-muted-foreground text-sm italic">
              No readme yet.
            </p>
          )}
        </div>
      )}
    </div>
  );
}
