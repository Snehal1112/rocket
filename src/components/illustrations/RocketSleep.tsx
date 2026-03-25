// Rocket sleeping with ZZZ and moon.
// Best for: idle/timeout states, inactive sessions.

interface Props { className?: string; }

export function RocketSleep({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      {/* Night sky glow. */}
      <circle cx="100" cy="105" r="75" className="fill-primary/5" />

      {/* Moon. */}
      <circle cx="155" cy="45" r="20" className="fill-muted-foreground/15" />
      <circle cx="162" cy="38" r="18" className="fill-background" />

      {/* Sleeping rocket — slightly tilted. */}
      <g transform="translate(95, 110) rotate(15)">
        <path d="M0 -45 C-10 -25, -13 0, -12 25 L-11 42 H11 L12 25 C13 0, 10 -25, 0 -45Z" className="fill-primary" />
        <path d="M0 -43 C-6 -25, -8 0, -8 22 L-7 38 H-1 L-2 22 C-2 0, -3 -25, 0 -43Z" className="fill-white/15" />
        <circle cx="0" cy="-8" r="8" className="fill-primary-foreground/80" />
        <circle cx="0" cy="-8" r="6" className="fill-background/60" />
        {/* Closed eyes (sleeping). */}
        <line x1="-3" y1="-8" x2="3" y2="-8" className="stroke-muted-foreground/40" strokeWidth="1.5" strokeLinecap="round" />
        <path d="M-12 20 C-18 26, -21 35, -19 42 L-11 38 L-12 20Z" className="fill-primary" />
        <path d="M12 20 C18 26, 21 35, 19 42 L11 38 L12 20Z" className="fill-primary" />
        <rect x="-7" y="38" width="14" height="5" rx="1.5" className="fill-muted-foreground/35" />
      </g>

      {/* ZZZ text. */}
      <text x="130" y="75" className="fill-primary/40" fontSize="18" fontFamily="sans-serif" fontWeight="700">Z</text>
      <text x="142" y="62" className="fill-primary/30" fontSize="14" fontFamily="sans-serif" fontWeight="700">Z</text>
      <text x="150" y="50" className="fill-primary/20" fontSize="10" fontFamily="sans-serif" fontWeight="700">Z</text>

      {/* Stars. */}
      <circle cx="35" cy="40" r="1.5" className="fill-primary/25" />
      <circle cx="50" cy="65" r="1" className="fill-primary/20" />
      <circle cx="25" cy="90" r="1.5" className="fill-primary/15" />
      <circle cx="170" cy="100" r="1" className="fill-primary/20" />
      <circle cx="45" cy="150" r="1.5" className="fill-primary/15" />
      <circle cx="160" cy="145" r="1" className="fill-primary/20" />

      {/* Ground. */}
      <path d="M30 170 Q65 164, 100 168 Q135 172, 170 166" className="stroke-muted-foreground/15" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}
