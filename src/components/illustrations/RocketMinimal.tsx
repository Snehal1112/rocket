// Minimal flat rocket matching the app's "Ready for liftoff" design.
// Clean geometric style with blue accents on a neutral body.
// Best for: response empty state, "send a request" prompts.

interface Props {
  className?: string;
}

export function RocketMinimal({ className = '' }: Props) {
  return (
    <svg
      viewBox='0 0 120 160'
      fill='none'
      xmlns='http://www.w3.org/2000/svg'
      className={className}
      aria-hidden='true'
      focusable='false'
    >
      {/* Shadow / ground reflection. */}
      <ellipse cx='60' cy='152' rx='22' ry='4' className='fill-muted-foreground/10' />

      {/* Left fin. */}
      <path d='M38 105 C28 112, 24 128, 28 138 L44 128 L42 105Z' className='fill-primary' />
      {/* Left fin shadow. */}
      <path d='M42 105 L44 128 L36 133 C30 125, 30 115, 38 105Z' className='fill-primary/80' />

      {/* Right fin. */}
      <path d='M82 105 C92 112, 96 128, 92 138 L76 128 L78 105Z' className='fill-primary' />

      {/* Rocket body — main shape. */}
      <path
        d='M60 8 C48 30, 42 60, 43 95 L44 130 H76 L77 95 C78 60, 72 30, 60 8Z'
        className='fill-muted-foreground/50'
      />

      {/* Body left highlight — lighter side. */}
      <path
        d='M60 8 C52 25, 47 52, 46 85 L45 125 H58 L57 85 C56 52, 57 25, 60 8Z'
        className='fill-muted-foreground/60'
      />

      {/* Body right shadow — darker side. */}
      <path
        d='M60 12 C67 30, 72 55, 73 88 L74 125 H63 L62 88 C62 55, 63 30, 60 12Z'
        className='fill-muted-foreground/35'
      />

      {/* Nose cone — blue tip. */}
      <path
        d='M60 8 C55 20, 51 32, 49 44 L60 44 L71 44 C69 32, 65 20, 60 8Z'
        className='fill-primary'
      />
      {/* Nose cone highlight. */}
      <path
        d='M60 8 C56 18, 53 28, 51 38 L58 38 C57 28, 58 18, 60 8Z'
        className='fill-primary/70'
      />

      {/* Window / porthole. */}
      <circle cx='60' cy='68' r='11' className='fill-muted-foreground/70' />
      <circle cx='60' cy='68' r='9' className='fill-background/70' />
      {/* Window inner ring. */}
      <circle cx='60' cy='68' r='7' className='fill-muted-foreground/15' />
      {/* Window reflection. */}
      <ellipse cx='57' cy='65' rx='2.5' ry='3.5' className='fill-white/15' />

      {/* Body stripe / band. */}
      <rect x='45' y='92' width='30' height='4' rx='1' className='fill-primary/30' />

      {/* Nozzle. */}
      <path d='M49 128 L47 140 H73 L71 128Z' className='fill-muted-foreground/40' />
      <path d='M51 128 L50 137 H70 L69 128Z' className='fill-muted-foreground/25' />

      {/* Small exhaust hint. */}
      <ellipse cx='60' cy='143' rx='6' ry='3' className='fill-muted-foreground/8' />
    </svg>
  );
}
