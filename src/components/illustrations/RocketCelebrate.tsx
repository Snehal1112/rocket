// Rocket with confetti and stars — celebration.
// Best for: success, completion, achievement states.

interface Props { className?: string; }

export function RocketCelebrate({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Confetti pieces. */}
      <rect x="40" y="50" width="6" height="3" rx="1" transform="rotate(30, 43, 51)" className="fill-primary/40" />
      <rect x="155" y="55" width="5" height="3" rx="1" transform="rotate(-20, 157, 56)" className="fill-orange-400/40" />
      <rect x="50" y="90" width="5" height="3" rx="1" transform="rotate(55, 52, 91)" className="fill-yellow-400/40" />
      <rect x="150" y="85" width="6" height="3" rx="1" transform="rotate(-45, 153, 86)" className="fill-emerald-400/40" />
      <rect x="35" y="130" width="5" height="3" rx="1" transform="rotate(15, 37, 131)" className="fill-purple-400/35" />
      <rect x="165" y="120" width="5" height="3" rx="1" transform="rotate(-35, 167, 121)" className="fill-primary/35" />
      <rect x="60" y="150" width="4" height="2.5" rx="0.8" transform="rotate(40, 62, 151)" className="fill-orange-400/30" />
      <rect x="140" y="145" width="5" height="2.5" rx="0.8" transform="rotate(-25, 142, 146)" className="fill-yellow-400/35" />

      {/* Confetti circles. */}
      <circle cx="45" cy="70" r="2.5" className="fill-emerald-400/30" />
      <circle cx="160" cy="70" r="2" className="fill-purple-400/25" />
      <circle cx="55" cy="110" r="2" className="fill-orange-400/25" />
      <circle cx="148" cy="105" r="2.5" className="fill-primary/30" />

      {/* Rocket. */}
      <g transform="translate(100, 95)">
        {/* Flame. */}
        <path d="M-4 32 C-6 40, -2 46, 0 42 C2 46, 6 40, 4 32" className="fill-orange-400/70" />
        <path d="M-2 32 C-3 37, -0.5 41, 0 39 C0.5 41, 3 37, 2 32" className="fill-yellow-400/80" />

        {/* Body. */}
        <path d="M0 -38 C-9 -20, -11 2, -11 20 L-10 30 H10 L11 20 C11 2, 9 -20, 0 -38Z" className="fill-primary" />
        <path d="M0 -36 C-5 -20, -7 2, -7 18 L-6 27 H-1 L-1.5 18 C-1.5 2, -2 -20, 0 -36Z" className="fill-white/20" />

        {/* Window. */}
        <circle cx="0" cy="-5" r="7" className="fill-primary-foreground/85" />
        <circle cx="0" cy="-5" r="5.5" className="fill-background/60" />
        <ellipse cx="-1.5" cy="-7" rx="1.5" ry="2" className="fill-white/30" />

        {/* Fins. */}
        <path d="M-11 16 C-16 22, -18 30, -16 34 L-10 30 L-11 16Z" className="fill-primary" />
        <path d="M11 16 C16 22, 18 30, 16 34 L10 30 L11 16Z" className="fill-primary" />

        {/* Nozzle. */}
        <rect x="-6" y="28" width="12" height="4" rx="1.2" className="fill-muted-foreground/35" />
      </g>

      {/* Star bursts. */}
      <g className="fill-yellow-400/50">
        <path d="M70 45 L72 40 L74 45 L72 50Z" />
        <path d="M70 45 L65 47 L70 49 L75 47Z" />
      </g>
      <g className="fill-yellow-400/40">
        <path d="M135 40 L136.5 36 L138 40 L136.5 44Z" />
        <path d="M135 40 L131 41.5 L135 43 L139 41.5Z" />
      </g>
      <g className="fill-primary/30">
        <path d="M40 155 L41 152 L42 155 L41 158Z" />
        <path d="M40 155 L37 156 L40 157 L43 156Z" />
      </g>
      <g className="fill-emerald-400/30">
        <path d="M162 150 L163 147 L164 150 L163 153Z" />
        <path d="M162 150 L159 151 L162 152 L165 151Z" />
      </g>
    </svg>
  );
}
