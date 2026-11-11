import type { LinkState, NetworkNode, NetworkNodeStatus } from '@/lib/tauri-api';

export const DEFAULT_NETWORK_PORT = '47821';

// Case, spaces and dashes do not matter, so "k7qf 2m" finds K7QF-2M9X.
const normalize = (text: string): string => text.toLowerCase().replace(/[\s-]/g, '');

/** Keeps the nodes whose name, initials or short id contain the query. */
export function filterNodes(nodes: NetworkNode[], query: string): NetworkNode[] {
  const needle = normalize(query);
  if (!needle) return nodes;
  return nodes.filter((node) =>
    [node.displayName, node.initials, node.shortId].some((field) =>
      normalize(field).includes(needle),
    ),
  );
}

const RANK: Record<NetworkNodeStatus, number> = {
  online: 0,
  pairing: 1,
  needs_update: 2,
  available: 3,
  offline: 4,
};

/** Online first, offline last, then by name and short id. The input is not changed. */
export function sortNodes(nodes: NetworkNode[]): NetworkNode[] {
  return [...nodes].sort(
    (a, b) =>
      RANK[a.status] - RANK[b.status] ||
      a.displayName.localeCompare(b.displayName) ||
      a.shortId.localeCompare(b.shortId),
  );
}

export function groupNodes(nodes: NetworkNode[]): { paired: NetworkNode[]; nearby: NetworkNode[] } {
  return {
    paired: nodes.filter((n) => n.group === 'paired'),
    nearby: nodes.filter((n) => n.group === 'nearby'),
  };
}

export type AddressCheck = { ok: true; host: string; port: number } | { ok: false; error: string };

const PORT_ERROR = 'Port must be a number from 1 to 65535.';

/** Checks the Add by address form. The backend still validates the host. */
export function validateAddress(hostInput: string, portInput: string): AddressCheck {
  const host = hostInput.trim();
  if (!host) return { ok: false, error: 'Enter a host name or IP address.' };
  if (/\s|\//.test(host)) {
    return { ok: false, error: 'Enter only the host, without spaces or a path.' };
  }
  const portText = portInput.trim();
  if (!/^\d{1,5}$/.test(portText)) return { ok: false, error: PORT_ERROR };
  const port = Number(portText);
  if (port < 1 || port > 65535) return { ok: false, error: PORT_ERROR };
  return { ok: true, host, port };
}

export const DISPLAY_NAME_MAX_CHARS = 40;

export type NameCheck = { ok: true; name: string } | { ok: false; error: string };

// Counts code points, like the backend counts chars, so an emoji is one character.
const hasControlChar = (text: string): boolean =>
  [...text].some((char) => {
    const code = char.codePointAt(0) ?? 0;
    return code < 32 || code === 127;
  });

/** Checks the display name before it goes to the backend. */
export function validateDisplayName(input: string): NameCheck {
  const name = input.trim();
  const length = [...name].length;
  if (length === 0) return { ok: false, error: 'Enter a name.' };
  if (length > DISPLAY_NAME_MAX_CHARS) {
    return { ok: false, error: `Use ${DISPLAY_NAME_MAX_CHARS} characters or fewer.` };
  }
  if (hasControlChar(name)) {
    return { ok: false, error: 'The name cannot contain control characters.' };
  }
  return { ok: true, name };
}

/** Dot colour per link state. A state added later gets the neutral dot. */
export function linkDotClass(state: LinkState): string {
  switch (state) {
    case 'in_sync':
      return 'bg-green-500';
    case 'changes_waiting':
    case 'out_of_date':
      return 'bg-amber-500';
    case 'broken':
      return 'bg-destructive';
    default:
      return 'bg-muted-foreground';
  }
}
