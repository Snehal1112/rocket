// Multiple small rockets flying together.
// Best for: team/collaboration empty, workspace sharing.

interface Props { className?: string; }

export function RocketGroup({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Lead rocket (center, larger). */}
      <g transform="translate(100, 85)">
        <path d="M-2 18 C-3 22, -1 26, 0 24 C1 26, 3 22, 2 18" className="fill-orange-400/70" />
        <path d="M0 -16 C-5 -6, -7 4, -7 14 H7 C7 4, 5 -6, 0 -16Z" className="fill-primary" />
        <path d="M0 -15 C-3 -6, -4 4, -4 12 H-1 C-1 4, -1.5 -6, 0 -15Z" className="fill-white/20" />
        <circle cx="0" cy="1" r="3.5" className="fill-background/60" />
        <ellipse cx="-0.8" cy="-0.5" rx="1" ry="1.3" className="fill-white/25" />
        <path d="M-7 10 L-10 16 L-7 14Z" className="fill-primary" />
        <path d="M7 10 L10 16 L7 14Z" className="fill-primary" />
        <rect x="-4" y="14" width="8" height="3" rx="1" className="fill-muted-foreground/35" />
      </g>

      {/* Left rocket (smaller, behind). */}
      <g transform="translate(65, 105) scale(0.75)">
        <path d="M-1.5 15 C-2 18, -0.5 20, 0 19 C0.5 20, 2 18, 1.5 15" className="fill-orange-400/60" />
        <path d="M0 -14 C-4.5 -5, -6 3, -6 12 H6 C6 3, 4.5 -5, 0 -14Z" className="fill-primary/80" />
        <path d="M0 -13 C-2.5 -5, -3.5 3, -3.5 10 H-0.5 C-0.5 3, -1 -5, 0 -13Z" className="fill-white/15" />
        <circle cx="0" cy="1" r="3" className="fill-background/55" />
        <path d="M-6 8 L-8.5 14 L-6 12Z" className="fill-primary/80" />
        <path d="M6 8 L8.5 14 L6 12Z" className="fill-primary/80" />
        <rect x="-3.5" y="12" width="7" height="2.5" rx="0.8" className="fill-muted-foreground/30" />
      </g>

      {/* Right rocket (smaller, behind). */}
      <g transform="translate(140, 100) scale(0.75)">
        <path d="M-1.5 15 C-2 18, -0.5 20, 0 19 C0.5 20, 2 18, 1.5 15" className="fill-orange-400/60" />
        <path d="M0 -14 C-4.5 -5, -6 3, -6 12 H6 C6 3, 4.5 -5, 0 -14Z" className="fill-primary/80" />
        <path d="M0 -13 C-2.5 -5, -3.5 3, -3.5 10 H-0.5 C-0.5 3, -1 -5, 0 -13Z" className="fill-white/15" />
        <circle cx="0" cy="1" r="3" className="fill-background/55" />
        <path d="M-6 8 L-8.5 14 L-6 12Z" className="fill-primary/80" />
        <path d="M6 8 L8.5 14 L6 12Z" className="fill-primary/80" />
        <rect x="-3.5" y="12" width="7" height="2.5" rx="0.8" className="fill-muted-foreground/30" />
      </g>

      {/* Tiny trailing rocket (far behind). */}
      <g transform="translate(118, 130) scale(0.5)">
        <path d="M0 -12 C-4 -4, -5 3, -5 10 H5 C5 3, 4 -4, 0 -12Z" className="fill-primary/60" />
        <circle cx="0" cy="0" r="2.5" className="fill-background/50" />
        <rect x="-3" y="10" width="6" height="2" rx="0.7" className="fill-muted-foreground/25" />
      </g>

      {/* Speed lines. */}
      <line x1="100" y1="115" x2="100" y2="130" className="stroke-muted-foreground/15" strokeWidth="1" strokeLinecap="round" />
      <line x1="68" y1="125" x2="68" y2="138" className="stroke-muted-foreground/12" strokeWidth="1" strokeLinecap="round" />
      <line x1="135" y1="122" x2="135" y2="135" className="stroke-muted-foreground/12" strokeWidth="1" strokeLinecap="round" />

      {/* Stars. */}
      <circle cx="30" cy="40" r="1.5" className="fill-primary/25" />
      <circle cx="170" cy="50" r="1" className="fill-primary/20" />
      <circle cx="40" cy="160" r="1" className="fill-primary/15" />
      <circle cx="160" cy="160" r="1.5" className="fill-primary/18" />
    </svg>
  );
}
