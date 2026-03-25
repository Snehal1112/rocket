// Rocket with a clock.
// Best for: history empty state, time-related states.

interface Props { className?: string; }

export function RocketClock({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Clock. */}
      <g transform="translate(135, 120)">
        <circle cx="0" cy="0" r="25" className="fill-muted/50" />
        <circle cx="0" cy="0" r="23" className="stroke-muted-foreground/20" strokeWidth="1.5" />
        <circle cx="0" cy="0" r="22" className="fill-background/40" />
        {/* Hour marks. */}
        {[0, 30, 60, 90, 120, 150, 180, 210, 240, 270, 300, 330].map((angle) => (
          <line
            key={angle}
            x1="0" y1="-19" x2="0" y2="-17"
            transform={`rotate(${angle})`}
            className="stroke-muted-foreground/25"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
        ))}
        {/* Hour hand. */}
        <line x1="0" y1="2" x2="0" y2="-10" className="stroke-muted-foreground/40" strokeWidth="2" strokeLinecap="round" transform="rotate(60)" />
        {/* Minute hand. */}
        <line x1="0" y1="2" x2="0" y2="-15" className="stroke-muted-foreground/30" strokeWidth="1.5" strokeLinecap="round" transform="rotate(210)" />
        {/* Center dot. */}
        <circle cx="0" cy="0" r="2" className="fill-muted-foreground/35" />
      </g>

      {/* Rocket. */}
      <g transform="translate(75, 85)">
        <path d="M-2 25 C-3 29, -1 32, 0 30 C1 32, 3 29, 2 25" className="fill-orange-400/60" />
        <path d="M0 -30 C-8 -16, -10 0, -10 18 L-9 24 H9 L10 18 C10 0, 8 -16, 0 -30Z" className="fill-primary" />
        <path d="M0 -28 C-5 -16, -6 0, -6 16 L-5.5 21 H-1 L-1.5 16 C-1.5 0, -2 -16, 0 -28Z" className="fill-white/18" />
        <circle cx="0" cy="-3" r="6" className="fill-primary-foreground/80" />
        <circle cx="0" cy="-3" r="4.5" className="fill-background/60" />
        <ellipse cx="-1" cy="-4.5" rx="1.2" ry="1.8" className="fill-white/25" />
        <path d="M-10 14 C-14 19, -16 26, -14 30 L-9 26 L-10 14Z" className="fill-primary" />
        <path d="M10 14 C14 19, 16 26, 14 30 L9 26 L10 14Z" className="fill-primary" />
        <rect x="-5.5" y="22" width="11" height="3.5" rx="1" className="fill-muted-foreground/35" />
      </g>

      {/* Stars. */}
      <circle cx="30" cy="40" r="1.5" className="fill-primary/25" />
      <circle cx="170" cy="45" r="1" className="fill-primary/20" />
      <circle cx="40" cy="160" r="1" className="fill-primary/15" />
      <circle cx="165" cy="165" r="1.5" className="fill-primary/18" />
    </svg>
  );
}
