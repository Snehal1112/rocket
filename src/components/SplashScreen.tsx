import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface SplashScreenProps {
  onComplete: () => void;
}

export function SplashScreen({ onComplete }: SplashScreenProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [fading, setFading] = useState(false);
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
      const lottie = (window as any).lottie;
      if (!lottie || !containerRef.current) return;

      const anim = lottie.loadAnimation({
        container: containerRef.current,
        renderer: 'svg',
        loop: false,
        autoplay: true,
        path: '/rocket-launch.json',
      });

      anim.addEventListener('complete', () => {
        setFading(true);
        setTimeout(onComplete, 500);
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
        'fixed inset-0 z-50 flex flex-col items-center justify-center bg-background transition-opacity duration-500',
        fading ? 'opacity-0' : 'opacity-100',
      )}
    >
      <div ref={containerRef} className="w-[300px] h-[300px] overflow-hidden" />
      <p className="mt-4 text-2xl font-bold text-foreground tracking-tight">
        Rocket
      </p>
    </div>
  );
}
