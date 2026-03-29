import Editor, { type OnMount } from '@monaco-editor/react';
import { BASE_EDITOR_OPTIONS, READONLY_OPTIONS, detectLanguage } from './monaco-config';
import { useMonacoTheme } from './useMonacoTheme';
import { EditorSkeleton } from './EditorSkeleton';

interface MonacoWrapperProps {
  value: string;
  onChange?: (value: string) => void;
  language?: string;
  bodyMode?: string;
  contentType?: string;
  readOnly?: boolean;
  height?: string;
}

export function MonacoWrapper({
  value,
  onChange,
  language,
  bodyMode,
  contentType,
  readOnly = false,
  height = '300px',
}: MonacoWrapperProps) {
  const { themeName, defineThemes } = useMonacoTheme();
  const resolvedLanguage = language ?? detectLanguage(bodyMode, contentType);
  const options = readOnly ? READONLY_OPTIONS : BASE_EDITOR_OPTIONS;

  const handleMount: OnMount = (_editor, monaco) => {
    defineThemes(monaco);
  };

  return (
    <Editor
      height={height}
      language={resolvedLanguage}
      value={value}
      onChange={(val) => onChange?.(val ?? '')}
      onMount={handleMount}
      theme={themeName}
      options={options}
      loading={<EditorSkeleton />}
    />
  );
}
