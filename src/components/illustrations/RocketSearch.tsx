// Rocket with a magnifying glass — searching through space.
// Best for: "no results found", search empty states, history empty.

interface Props {
  className?: string;
}

export function RocketSearch({ className = '' }: Props) {
  return (
    <svg
      viewBox="0 0 200 200"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      {/* Background glow. */}
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Magnifying glass. */}
      {/* Handle. */}
      <line
        x1="138" y1="138" x2="160" y2="160"
        className="stroke-muted-foreground/40"
        strokeWidth="6"
        strokeLinecap="round"
      />
      {/* Lens ring. */}
      <circle cx="120" cy="120" r="30" className="stroke-muted-foreground/30" strokeWidth="4" />
      <circle cx="120" cy="120" r="28" className="fill-primary/5" />
      {/* Lens glare. */}
      <ellipse cx="110" cy="112" rx="8" ry="10" className="fill-white/10" />

      {/* Mini rocket inside the magnifying glass — tilted, flying. */}
      <g transform="translate(120, 115) rotate(-30)">
        {/* Flame. */}
        <path d="M-1.5 14 C-2.5 17, -0.5 20, 0 18 C0.5 20, 2.5 17, 1.5 14" className="fill-orange-400/70" />
        <path d="M-0.8 14 C-1.2 16, 0 17, 0 16.5 C0 17, 1.2 16, 0.8 14" className="fill-yellow-400/80" />

        {/* Body. */}
        <path d="M0 -10 C-3.5 -3, -4.5 3, -4.5 10 H4.5 C4.5 3, 3.5 -3, 0 -10Z" className="fill-primary" />
        <path d="M0 -10 C-2 -3, -2.5 3, -2.5 8 H-0.5 C-0.5 3, -1 -3, 0 -10Z" className="fill-white/20" />

        {/* Window. */}
        <circle cx="0" cy="0" r="2.5" className="fill-background/60" />
        <ellipse cx="-0.5" cy="-0.6" rx="0.8" ry="1" className="fill-white/30" />

        {/* Fins. */}
        <path d="M-4.5 7 L-7 12 L-4.5 10Z" className="fill-primary" />
        <path d="M4.5 7 L7 12 L4.5 10Z" className="fill-primary" />

        {/* Nozzle. */}
        <rect x="-2.5" y="10" width="5" height="2.5" rx="0.8" className="fill-muted-foreground/40" />
      </g>

      {/* Scattered stars outside the lens. */}
      <circle cx="30" cy="45" r="2" className="fill-primary/35" />
      <circle cx="55" cy="25" r="1.5" className="fill-primary/25" />
      <circle cx="170" cy="50" r="1.5" className="fill-primary/20" />
      <circle cx="20" cy="110" r="1" className="fill-primary/15" />
      <circle cx="45" cy="160" r="2" className="fill-primary/25" />
      <circle cx="70" cy="45" r="1" className="fill-primary/20" />
      <circle cx="175" cy="90" r="1.5" className="fill-primary/20" />

      {/* Question marks scattered — "searching". */}
      <text x="50" y="80" className="fill-muted-foreground/20" fontSize="16" fontFamily="sans-serif" fontWeight="600">?</text>
      <text x="160" y="75" className="fill-muted-foreground/15" fontSize="12" fontFamily="sans-serif" fontWeight="600">?</text>
      <text x="35" y="140" className="fill-muted-foreground/15" fontSize="14" fontFamily="sans-serif" fontWeight="600">?</text>
    </svg>
  );
}
