// Rocket landing on a planet.
// Best for: destination reached, success, completed states.

interface Props { className?: string; }

export function RocketPlanet({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Planet surface (curved horizon). */}
      <path d="M10 155 Q100 120, 190 155 L190 200 H10Z" className="fill-muted/40" />
      <path d="M10 155 Q100 120, 190 155" className="stroke-muted-foreground/15" strokeWidth="1" />
      {/* Craters. */}
      <ellipse cx="60" cy="168" rx="10" ry="4" className="fill-muted-foreground/8" />
      <ellipse cx="145" cy="172" rx="8" ry="3" className="fill-muted-foreground/6" />
      <ellipse cx="100" cy="178" rx="6" ry="2.5" className="fill-muted-foreground/5" />

      {/* Rocket descending — slight tilt. */}
      <g transform="translate(100, 80) rotate(5)">
        {/* Small landing flame. */}
        <path d="M-3 30 C-4 34, -1 37, 0 35 C1 37, 4 34, 3 30" className="fill-orange-400/50" />

        {/* Body. */}
        <path d="M0 -32 C-8 -16, -10 2, -10 18 L-9 28 H9 L10 18 C10 2, 8 -16, 0 -32Z" className="fill-primary" />
        <path d="M0 -30 C-5 -16, -6 2, -6 16 L-5.5 25 H-1 L-1.5 16 C-1.5 2, -2 -16, 0 -30Z" className="fill-white/18" />

        {/* Window. */}
        <circle cx="0" cy="-3" r="6" className="fill-primary-foreground/80" />
        <circle cx="0" cy="-3" r="4.5" className="fill-background/60" />
        <ellipse cx="-1" cy="-5" rx="1.2" ry="1.8" className="fill-white/25" />

        {/* Fins / landing legs. */}
        <path d="M-10 16 L-15 30 L-9 26Z" className="fill-primary" />
        <path d="M10 16 L15 30 L9 26Z" className="fill-primary" />

        {/* Nozzle. */}
        <rect x="-5.5" y="26" width="11" height="3.5" rx="1" className="fill-muted-foreground/35" />
      </g>

      {/* Flag on planet. */}
      <line x1="135" y1="140" x2="135" y2="155" className="stroke-muted-foreground/30" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M135 140 L148 144 L135 148Z" className="fill-primary/40" />

      {/* Stars. */}
      <circle cx="30" cy="35" r="2" className="fill-primary/30" />
      <circle cx="170" cy="30" r="1.5" className="fill-primary/25" />
      <circle cx="25" cy="80" r="1" className="fill-primary/15" />
      <circle cx="175" cy="75" r="1.5" className="fill-primary/20" />
      <circle cx="50" cy="55" r="1" className="fill-primary/18" />
      <circle cx="155" cy="55" r="1.5" className="fill-primary/22" />
    </svg>
  );
}
