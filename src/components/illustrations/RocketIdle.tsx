// Rocket sitting on a launch pad, ready to go.
// Best for: "ready to start" states, onboarding, first-time user prompts.

interface Props {
  className?: string;
}

export function RocketIdle({ className = '' }: Props) {
  return (
    <svg
      viewBox="0 0 200 200"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      {/* Background glow. */}
      <circle cx="100" cy="95" r="70" className="fill-primary/5" />

      {/* Launch pad base. */}
      <rect x="40" y="168" width="120" height="6" rx="3" className="fill-muted-foreground/25" />
      <rect x="55" y="164" width="90" height="4" rx="2" className="fill-muted-foreground/15" />

      {/* Launch pad legs. */}
      <rect x="70" y="155" width="4" height="13" rx="1" className="fill-muted-foreground/30" />
      <rect x="126" y="155" width="4" height="13" rx="1" className="fill-muted-foreground/30" />

      {/* Rocket body. */}
      <path
        d="M100 28 C87 50, 81 78, 83 110 L85 150 H115 L117 110 C119 78, 113 50, 100 28Z"
        className="fill-primary"
      />

      {/* Body highlight. */}
      <path
        d="M100 32 C92 50, 87 75, 88 108 L89 145 H96 L95 108 C94 75, 96 50, 100 32Z"
        className="fill-white/20"
      />

      {/* Stripe band. */}
      <path
        d="M84 115 L83.5 125 H116.5 L116 115 C112 114, 88 114, 84 115Z"
        className="fill-accent/50"
      />

      {/* Window. */}
      <circle cx="100" cy="75" r="13" className="fill-primary-foreground/90" />
      <circle cx="100" cy="75" r="10" className="fill-background/60" />
      <ellipse cx="97" cy="72" rx="3.5" ry="4.5" className="fill-white/30" />

      {/* Second smaller window. */}
      <circle cx="100" cy="100" r="6" className="fill-primary-foreground/70" />
      <circle cx="100" cy="100" r="4.5" className="fill-background/50" />

      {/* Left fin. */}
      <path d="M83 120 C73 130, 66 145, 68 158 L85 150 L83 120Z" className="fill-primary" />
      <path d="M83 120 C77 130, 72 142, 70 152 L78 147 L83 120Z" className="fill-white/15" />

      {/* Right fin. */}
      <path d="M117 120 C127 130, 134 145, 132 158 L115 150 L117 120Z" className="fill-primary" />

      {/* Nozzle. */}
      <rect x="88" y="147" width="24" height="8" rx="2" className="fill-muted-foreground/40" />
      <rect x="91" y="150" width="18" height="5" rx="1.5" className="fill-muted-foreground/25" />

      {/* Smoke puffs at base. */}
      <circle cx="80" cy="165" r="6" className="fill-muted/60" />
      <circle cx="70" cy="162" r="4" className="fill-muted/40" />
      <circle cx="120" cy="165" r="6" className="fill-muted/60" />
      <circle cx="130" cy="162" r="4" className="fill-muted/40" />
      <circle cx="100" cy="163" r="5" className="fill-muted/50" />

      {/* Stars. */}
      <circle cx="35" cy="40" r="2" className="fill-primary/35" />
      <circle cx="165" cy="35" r="1.5" className="fill-primary/25" />
      <circle cx="25" cy="95" r="1.5" className="fill-primary/20" />
      <circle cx="175" cy="85" r="2" className="fill-primary/30" />
      <circle cx="150" cy="50" r="1" className="fill-primary/20" />
      <circle cx="50" cy="30" r="1" className="fill-primary/15" />

      {/* Twinkle star. */}
      <g className="fill-primary/30">
        <path d="M160 55 L161 52 L162 55 L161 58Z" />
        <path d="M160 55 L157 56 L160 57 L163 56Z" />
      </g>
    </svg>
  );
}
