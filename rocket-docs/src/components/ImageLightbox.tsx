import type { ImgHTMLAttributes } from 'react';
import { Dialog as DialogPrimitive } from '@base-ui/react/dialog';
import { XIcon } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogClose, DialogOverlay, DialogPortal, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { cn } from '@/lib/utils';

interface ImageLightboxProps {
  src: string;
  alt: string;
  className?: string;
  loading?: ImgHTMLAttributes<HTMLImageElement>['loading'];
  onError?: () => void;
}

/** A thumbnail that opens the same image enlarged, in a dialog, on click. */
export function ImageLightbox({ src, alt, className, loading = 'lazy', onError }: ImageLightboxProps) {
  return (
    <Dialog>
      <DialogTrigger
        render={
          <button
            type="button"
            aria-label={`Open larger view: ${alt}`}
            className="block w-full cursor-zoom-in appearance-none rounded-lg bg-transparent p-0 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
          />
        }
      >
        <img src={src} alt={alt} loading={loading} decoding="async" onError={onError} className={className} />
      </DialogTrigger>
      <DialogPortal>
        <DialogOverlay />
        <DialogPrimitive.Popup
          data-slot="dialog-content"
          className={cn(
            'fixed top-1/2 left-1/2 z-50 w-full max-w-[min(92vw,1400px)] -translate-x-1/2 -translate-y-1/2 outline-none',
            'data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95',
            'data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95 data-closed:fill-mode-forwards',
          )}
        >
          <DialogTitle className="sr-only">{alt}</DialogTitle>
          <img
            src={src}
            alt={alt}
            className="mx-auto block max-h-[85vh] w-auto max-w-full rounded-xl border border-border shadow-2xl"
          />
          <DialogClose
            render={
              <Button
                variant="ghost"
                size="icon"
                className="absolute -top-3 -right-3 bg-secondary shadow-md"
              />
            }
          >
            <XIcon />
            <span className="sr-only">Close</span>
          </DialogClose>
        </DialogPrimitive.Popup>
      </DialogPortal>
    </Dialog>
  );
}
