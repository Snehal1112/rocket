/** Deterministic avatar color from party name/seed. Cycles through 7 brand colors. */
const AVATAR_PALETTE = [
  '#3B82F6', // blue
  '#A855F7', // purple
  '#22C55E', // green
  '#F59E0B', // amber
  '#EC4899', // pink
  '#14B8A6', // teal
  '#EF4444', // red
];

function hashString(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return Math.abs(h);
}

export function avatarColorForName(name: string, override?: string): string {
  if (override) return override;
  return AVATAR_PALETTE[hashString(name) % AVATAR_PALETTE.length];
}

export function initialsForName(name: string): string {
  const words = name.trim().split(/\s+/);
  if (words.length >= 2) return (words[0][0] + words[1][0]).toUpperCase();
  return name.slice(0, 2).toUpperCase();
}
