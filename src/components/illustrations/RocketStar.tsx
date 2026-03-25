// Rocket circling a star.
// Best for: favorites empty, starred items empty.

interface Props { className?: string; }

export function RocketStar({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Central star. */}
      <path
        d="M100 55 L107 80 L133 80 L112 95 L119 120 L100 105 L81 120 L88 95 L67 80 L93 80Z"
        className="fill-yellow-400/50"
      />
      <path
        d="M100 62 L105 80 L124 80 L109 91 L114 110 L100 100 L86 110 L91 91 L76 80 L95 80Z"
        className="fill-yellow-400/70"
      />

      {/* Orbital ring. */}
      <ellipse cx="100" cy="90" rx="60" ry="16" className="stroke-primary/15" strokeWidth="1.5" strokeDasharray="4 3" transform="rotate(-10, 100, 90)" />

      {/* Mini rocket on orbit. */}
      <g transform="translate(155, 72) rotate(50)">
        <path d="M-1.5 12 C-2 15, -0.5 17, 0 16 C0.5 17, 2 15, 1.5 12" className="fill-orange-400/60" />
        <path d="M0 -9 C-3 -3, -4 3, -4 9 H4 C4 3, 3 -3, 0 -9Z" className="fill-primary" />
        <path d="M0 -9 C-1.5 -3, -2 3, -2 7 H0 C0 3, -0.5 -3, 0 -9Z" className="fill-white/20" />
        <circle cx="0" cy="0" r="2.2" className="fill-background/60" />
        <path d="M-4 6 L-6 10 L-4 9Z" className="fill-primary" />
        <path d="M4 6 L6 10 L4 9Z" className="fill-primary" />
        <rect x="-2.2" y="9" width="4.4" height="2" rx="0.7" className="fill-muted-foreground/35" />
      </g>

      {/* Small stars. */}
      <circle cx="35" cy="45" r="2" className="fill-primary/30" />
      <circle cx="165" cy="40" r="1.5" className="fill-primary/20" />
      <circle cx="30" cy="130" r="1" className="fill-primary/15" />
      <circle cx="170" cy="135" r="1.5" className="fill-primary/20" />
      <circle cx="55" cy="165" r="1" className="fill-primary/15" />
      <circle cx="145" cy="160" r="1.5" className="fill-primary/18" />

      {/* Twinkle. */}
      <g className="fill-yellow-400/30">
        <path d="M40 80 L41 77 L42 80 L41 83Z" />
        <path d="M40 80 L37 81 L40 82 L43 81Z" />
      </g>
      <g className="fill-yellow-400/20">
        <path d="M160 110 L161 108 L162 110 L161 112Z" />
        <path d="M160 110 L158 111 L160 112 L162 111Z" />
      </g>
    </svg>
  );
}
