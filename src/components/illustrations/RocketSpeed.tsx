// Rocket with motion blur and speed lines.
// Best for: performance, fast response, speed-related states.

interface Props { className?: string; }

export function RocketSpeed({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Long speed lines behind rocket. */}
      <line x1="20" y1="130" x2="65" y2="130" className="stroke-primary/15" strokeWidth="2" strokeLinecap="round" />
      <line x1="30" y1="110" x2="60" y2="110" className="stroke-primary/12" strokeWidth="1.5" strokeLinecap="round" />
      <line x1="15" y1="145" x2="55" y2="145" className="stroke-primary/10" strokeWidth="1.5" strokeLinecap="round" />
      <line x1="25" y1="95" x2="50" y2="95" className="stroke-primary/8" strokeWidth="1" strokeLinecap="round" />
      <line x1="35" y1="160" x2="60" y2="160" className="stroke-primary/8" strokeWidth="1" strokeLinecap="round" />
      <line x1="40" y1="80" x2="58" y2="80" className="stroke-primary/6" strokeWidth="1" strokeLinecap="round" />

      {/* Rocket — angled for speed. */}
      <g transform="translate(120, 100) rotate(-25)">
        {/* Big flame. */}
        <path d="M-5 35 C-8 45, -3 55, 0 50 C3 55, 8 45, 5 35" className="fill-orange-400/70" />
        <path d="M-3 35 C-5 42, -1 48, 0 45 C1 48, 5 42, 3 35" className="fill-yellow-400/80" />
        <path d="M-1.5 35 C-2 40, 0 44, 0 42 C0 44, 2 40, 1.5 35" className="fill-white/40" />

        {/* Body. */}
        <path d="M0 -38 C-9 -20, -12 0, -11 20 L-10 32 H10 L11 20 C12 0, 9 -20, 0 -38Z" className="fill-primary" />
        <path d="M0 -36 C-5 -20, -7 0, -7 18 L-6 28 H-1 L-2 18 C-2 0, -2.5 -20, 0 -36Z" className="fill-white/20" />

        {/* Window. */}
        <circle cx="0" cy="-5" r="7" className="fill-primary-foreground/85" />
        <circle cx="0" cy="-5" r="5.5" className="fill-background/60" />
        <ellipse cx="-1.5" cy="-7" rx="1.5" ry="2" className="fill-white/30" />

        {/* Fins. */}
        <path d="M-11 16 C-17 22, -19 30, -17 35 L-10 32 L-11 16Z" className="fill-primary" />
        <path d="M11 16 C17 22, 19 30, 17 35 L10 32 L11 16Z" className="fill-primary" />

        {/* Nozzle. */}
        <rect x="-6" y="30" width="12" height="4" rx="1.2" className="fill-muted-foreground/35" />
      </g>

      {/* Motion particles. */}
      <circle cx="55" cy="120" r="2" className="fill-primary/20" />
      <circle cx="50" cy="105" r="1.5" className="fill-primary/15" />
      <circle cx="60" cy="135" r="1" className="fill-primary/12" />

      {/* Stars. */}
      <circle cx="170" cy="40" r="1.5" className="fill-primary/25" />
      <circle cx="175" cy="155" r="1" className="fill-primary/18" />
    </svg>
  );
}
