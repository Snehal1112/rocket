// Rocket with an open book.
// Best for: documentation empty, help/guide states.

interface Props {
  className?: string;
}

export function RocketBook({ className = '' }: Props) {
  return (
    <svg
      viewBox='0 0 200 200'
      fill='none'
      xmlns='http://www.w3.org/2000/svg'
      className={className}
      aria-hidden='true'
      focusable='false'
    >
      {/* Background glow. */}
      <circle cx='100' cy='100' r='70' className='fill-primary/5' />

      {/* Left page. */}
      <path
        d='M22 148 C22 144, 26 141, 30 140 L95 136 L95 168 L30 172 C26 172, 22 169, 22 165Z'
        className='fill-muted/80'
      />
      {/* Right page. */}
      <path
        d='M178 148 C178 144, 174 141, 170 140 L105 136 L105 168 L170 172 C174 172, 178 169, 178 165Z'
        className='fill-muted/60'
      />
      {/* Book spine. */}
      <path
        d='M95 136 L95 168 L100 170 L105 168 L105 136 L100 134Z'
        className='fill-muted-foreground/30'
      />
      {/* Book bottom edge shadow. */}
      <path
        d='M22 165 C22 169, 26 172, 30 172 L95 168 L105 168 L170 172 C174 172, 178 169, 178 165 L178 168 C178 172, 174 175, 170 175 L30 175 C26 175, 22 172, 22 168Z'
        className='fill-muted-foreground/15'
      />

      {/* Left page lines. */}
      <line
        x1='35'
        y1='148'
        x2='88'
        y2='145'
        className='stroke-muted-foreground/20'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='35'
        y1='154'
        x2='88'
        y2='151'
        className='stroke-muted-foreground/20'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='35'
        y1='160'
        x2='88'
        y2='157'
        className='stroke-muted-foreground/15'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='35'
        y1='166'
        x2='75'
        y2='163'
        className='stroke-muted-foreground/10'
        strokeWidth='1.5'
        strokeLinecap='round'
      />

      {/* Right page lines. */}
      <line
        x1='112'
        y1='145'
        x2='165'
        y2='148'
        className='stroke-muted-foreground/15'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='112'
        y1='151'
        x2='165'
        y2='154'
        className='stroke-muted-foreground/15'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='112'
        y1='157'
        x2='165'
        y2='160'
        className='stroke-muted-foreground/10'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='112'
        y1='163'
        x2='150'
        y2='165'
        className='stroke-muted-foreground/8'
        strokeWidth='1.5'
        strokeLinecap='round'
      />

      {/* Rocket body. */}
      <path
        d='M100 22 C88 44, 82 72, 84 104 L86 138 H114 L116 104 C118 72, 112 44, 100 22Z'
        className='fill-primary'
      />

      {/* Body highlight. */}
      <path
        d='M100 26 C92 46, 87 71, 88 102 L89 133 H96 L95 102 C94 71, 96 46, 100 26Z'
        className='fill-white/20'
      />

      {/* Stripe band. */}
      <path
        d='M85 108 L84 120 H116 L115 108 C111 107, 89 107, 85 108Z'
        className='fill-accent/50'
      />

      {/* Main window. */}
      <circle cx='100' cy='70' r='14' className='fill-primary-foreground/90' />
      <circle cx='100' cy='70' r='11' className='fill-background/60' />
      <ellipse cx='97' cy='67' rx='4' ry='5' className='fill-white/30' />

      {/* Second window. */}
      <circle cx='100' cy='95' r='7' className='fill-primary-foreground/70' />
      <circle cx='100' cy='95' r='5.5' className='fill-background/50' />
      <ellipse cx='99' cy='94' rx='1.5' ry='2' className='fill-white/25' />

      {/* Left fin. */}
      <path d='M84 115 C74 125, 67 138, 69 152 L86 143 L84 115Z' className='fill-primary' />
      <path d='M84 115 C78 124, 73 136, 72 148 L80 142 L84 115Z' className='fill-white/15' />

      {/* Right fin. */}
      <path d='M116 115 C126 125, 133 138, 131 152 L114 143 L116 115Z' className='fill-primary' />

      {/* Nozzle. */}
      <rect x='88' y='135' width='24' height='9' rx='2.5' className='fill-muted-foreground/40' />
      <rect x='91' y='138' width='18' height='6' rx='1.5' className='fill-muted-foreground/25' />

      {/* Exhaust flame. */}
      <path
        d='M93 144 C91 150, 90 157, 93 162 C96 158, 100 155, 100 144Z'
        className='fill-orange-400/70'
      />
      <path
        d='M107 144 C109 150, 110 157, 107 162 C104 158, 100 155, 100 144Z'
        className='fill-orange-400/70'
      />
      <path
        d='M97 144 C96 152, 97 158, 100 160 C103 158, 104 152, 103 144Z'
        className='fill-yellow-300/60'
      />

      {/* Sparkle particles. */}
      <circle cx='72' cy='118' r='2' className='fill-primary/30' />
      <circle cx='128' cy='115' r='1.5' className='fill-primary/25' />
      <circle cx='65' cy='100' r='1.5' className='fill-primary/20' />
      <circle cx='135' cy='98' r='2' className='fill-primary/22' />

      {/* Stars. */}
      <circle cx='32' cy='38' r='2' className='fill-primary/35' />
      <circle cx='168' cy='33' r='1.5' className='fill-primary/25' />
      <circle cx='22' cy='92' r='1.5' className='fill-primary/20' />
      <circle cx='178' cy='82' r='2' className='fill-primary/30' />
      <circle cx='152' cy='48' r='1' className='fill-primary/20' />
      <circle cx='48' cy='28' r='1' className='fill-primary/15' />

      {/* Twinkle star. */}
      <g className='fill-primary/30'>
        <path d='M158 52 L159.5 48 L161 52 L159.5 56Z' />
        <path d='M155 54 L159.5 52.5 L164 54 L159.5 55.5Z' />
      </g>
    </svg>
  );
}
