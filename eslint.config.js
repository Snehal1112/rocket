import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";

export default tseslint.config(
  { ignores: ["dist", "src-tauri", "node_modules", ".worktrees"] },

  js.configs.recommended,
  ...tseslint.configs.recommended,

  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": [
        "warn",
        { allowConstantExport: true },
      ],

      // TypeScript handles these via noUnusedLocals / noUnusedParameters.
      "@typescript-eslint/no-unused-vars": "off",

      // Warn on explicit any — allow in edge cases but discourage.
      "@typescript-eslint/no-explicit-any": "warn",

      // Allow empty catch blocks (project convention).
      "@typescript-eslint/no-unused-expressions": "off",
      "no-empty": ["error", { allowEmptyCatch: true }],

      // Calling setState in effects is a common pattern for data fetching
      // and state synchronization in this codebase.
      "react-hooks/set-state-in-effect": "off",

      // React Compiler memoization rule is too strict for existing code.
      "react-hooks/preserve-manual-memoization": "off",
    },
  },
);
