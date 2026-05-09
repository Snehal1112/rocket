import { exportContractOpenapi } from '@/lib/tauri-api';

/**
 * Fetch the OpenAPI YAML for a contract from Rust, then prompt the user to
 * save it via the Tauri save-dialog plugin.  Falls back to a browser blob
 * download when the plugin is unavailable (e.g. running in the Vite dev
 * server without a Tauri context).
 */
export async function saveContractAsOpenApi(
  collectionRoot: string,
  contractId: string,
  contractName: string,
): Promise<void> {
  const yaml = await exportContractOpenapi(collectionRoot, contractId);
  const filename = `${contractName.replace(/\s+/g, '-').toLowerCase()}-openapi.yaml`;

  // Try Tauri dialog + fs plugins first (desktop app context).
  try {
    const [{ save }, { writeTextFile }] = await Promise.all([
      import('@tauri-apps/plugin-dialog'),
      import('@tauri-apps/plugin-fs'),
    ]);

    const savePath = await save({
      defaultPath: filename,
      filters: [{ name: 'YAML', extensions: ['yaml', 'yml'] }],
    });

    if (savePath) {
      await writeTextFile(savePath, yaml);
    }
    return;
  } catch {
    // Not in a Tauri context — fall through to browser download.
  }

  // Browser fallback: trigger an <a> download.
  const blob = new Blob([yaml], { type: 'text/yaml' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
