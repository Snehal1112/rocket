// Rocket carrying a folder.
// Best for: collections empty state, no saved requests.

interface Props { className?: string; }

export function RocketFolder({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Folder. */}
      <g transform="translate(100, 135)">
        {/* Folder back. */}
        <rect x="-30" y="-20" width="60" height="35" rx="3" className="fill-muted-foreground/20" />
        {/* Folder tab. */}
        <path d="M-30 -20 L-30 -26 H-10 L-5 -20Z" className="fill-muted-foreground/25" />
        {/* Folder front. */}
        <rect x="-30" y="-14" width="60" height="29" rx="2" className="fill-muted-foreground/15" />
        {/* Folder line. */}
        <line x1="-20" y1="-5" x2="20" y2="-5" className="stroke-muted-foreground/10" strokeWidth="1.5" />
        <line x1="-20" y1="2" x2="10" y2="2" className="stroke-muted-foreground/8" strokeWidth="1.5" />
      </g>

      {/* Rocket rising out of folder. */}
      <g transform="translate(100, 72)">
        {/* Flame. */}
        <path d="M-2.5 28 C-3.5 33, -1 36, 0 34 C1 36, 3.5 33, 2.5 28" className="fill-orange-400/60" />

        {/* Body. */}
        <path d="M0 -28 C-8 -14, -10 2, -10 18 L-9 26 H9 L10 18 C10 2, 8 -14, 0 -28Z" className="fill-primary" />
        <path d="M0 -26 C-5 -14, -6 2, -6 16 L-5.5 23 H-1 L-1.5 16 C-1.5 2, -2 -14, 0 -26Z" className="fill-white/18" />

        {/* Window. */}
        <circle cx="0" cy="-2" r="5.5" className="fill-primary-foreground/80" />
        <circle cx="0" cy="-2" r="4" className="fill-background/60" />
        <ellipse cx="-1" cy="-3.5" rx="1" ry="1.5" className="fill-white/25" />

        {/* Fins. */}
        <path d="M-10 14 C-15 19, -17 26, -15 30 L-9 26 L-10 14Z" className="fill-primary" />
        <path d="M10 14 C15 19, 17 26, 15 30 L9 26 L10 14Z" className="fill-primary" />

        {/* Nozzle. */}
        <rect x="-5.5" y="24" width="11" height="3.5" rx="1" className="fill-muted-foreground/35" />
      </g>

      {/* Sparkles. */}
      <circle cx="72" cy="100" r="1.5" className="fill-primary/25" />
      <circle cx="128" cy="98" r="1" className="fill-primary/20" />
      <circle cx="65" cy="85" r="1" className="fill-primary/15" />
      <circle cx="135" cy="82" r="1.5" className="fill-primary/18" />

      {/* Stars. */}
      <circle cx="35" cy="35" r="1.5" className="fill-primary/25" />
      <circle cx="165" cy="40" r="1" className="fill-primary/20" />
      <circle cx="170" cy="100" r="1.5" className="fill-primary/15" />
    </svg>
  );
}
