import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface SplashScreenProps {
  onComplete: () => void;
}

export function SplashScreen({ onComplete }: SplashScreenProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [liftingOff, setLiftingOff] = useState(false);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    requestAnimationFrame(() => setMounted(true));
  }, []);

  useEffect(() => {
    // Load lottie.min.js from public/ via a script tag.
    const script = document.createElement('script');
    script.src = '/lottie.min.js';
    script.async = true;

    script.onload = () => {
      const lottie = (
        window as {
          lottie?: {
            loadAnimation: (opts: {
              container: HTMLDivElement;
              renderer: string;
              loop: boolean;
              autoplay: boolean;
              path: string;
            }) => {
              addEventListener: (event: string, handler: () => void) => void;
              destroy: () => void;
            };
          };
        }
      ).lottie;
      if (!lottie || !containerRef.current) return;

      const anim = lottie.loadAnimation({
        container: containerRef.current,
        renderer: 'svg',
        loop: false,
        autoplay: true,
        path: '/rocket-launch.json',
      });

      anim.addEventListener('complete', () => {
        setLiftingOff(true);
        setTimeout(onComplete, 1000);
      });

      return () => anim.destroy();
    };

    document.head.appendChild(script);
    return () => {
      script.remove();
    };
  }, [onComplete]);

  return (
    <div
      className={cn(
        'fixed inset-0 z-50 flex flex-col border border- items-center justify-center bg-background',
        liftingOff ? 'opacity-0 transition-opacity duration-[400ms] delay-[600ms]' : 'opacity-100',
      )}
      data-tauri-drag-region
    >
      <div
        className={cn(
          mounted ? 'scale-100 opacity-100' : 'scale-95 opacity-0',
          liftingOff
            ? 'transition-transform duration-[800ms] ease-in -translate-y-[100vh]'
            : 'transition-all duration-300',
        )}
      >
        <div className='relative flex flex-col items-center'>
          {/* Radial glow behind rocket. */}
          <div className='absolute inset-0 flex items-center justify-center pointer-events-none'>
            <div className='w-[200px] h-[200px] rounded-full bg-blue-500/5 blur-2xl' />
          </div>
          {/* Lottie animation container. */}
          <div ref={containerRef} className='w-[160px] h-[160px] overflow-hidden relative z-10' />
          {/* App name and subtitle. */}
          <p className='mt-2 text-2xl font-bold text-foreground tracking-tight'>Rocket</p>
          <p className='text-sm text-muted-foreground tracking-wide'>API Workspace</p>
        </div>
      </div>
    </div>
  );
}
