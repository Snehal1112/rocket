// This module must be imported before any Monaco editor component mounts.
// It wires the locally-bundled monaco-editor to @monaco-editor/react and
// registers the custom Rocket themes so no flash of the default light theme
// occurs on first render, regardless of which editor component opens first.
import { loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import { defineMonacoThemes } from './monaco-config';

loader.config({ monaco });
defineMonacoThemes(monaco);
