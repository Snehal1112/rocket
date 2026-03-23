import { useCallback, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface ResizeHandleProps {
  direction: 'horizontal' | 'vertical';
  onResize: (sizes: [number, number]) => void;
}

// Thin draggable divider that recalculates split percentages on drag.
export function ResizeHandle({ direction, onResize }: ResizeHandleProps) {
  const isHorizontal = direction === 'horizontal';
  const handleRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsDragging(true);

      const parent = handleRef.current?.parentElement;
      if (!parent) return;

      const parentRect = parent.getBoundingClientRect();
      const totalSize = isHorizontal ? parentRect.width : parentRect.height;
      const parentStart = isHorizontal ? parentRect.left : parentRect.top;

      const onMouseMove = (moveEvent: MouseEvent) => {
        const cursor = isHorizontal ? moveEvent.clientX : moveEvent.clientY;
        let ratio = ((cursor - parentStart) / totalSize) * 100;

        // Clamp to prevent panels from collapsing completely.
        ratio = Math.max(10, Math.min(90, ratio));
        const newSizes: [number, number] = [ratio, 100 - ratio];
        onResize(newSizes);
      };

      const onMouseUp = () => {
        setIsDragging(false);
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
      };

      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    },
    [isHorizontal, onResize],
  );

  return (
    <div
      ref={handleRef}
      role="separator"
      aria-orientation={isHorizontal ? 'vertical' : 'horizontal'}
      tabIndex={0}
      onMouseDown={onMouseDown}
      className={cn(
        'flex-shrink-0 bg-border transition-colors',
        isHorizontal
          ? 'w-1 cursor-col-resize hover:w-1.5 hover:bg-primary/50'
          : 'h-1 cursor-row-resize hover:h-1.5 hover:bg-primary/50',
        isDragging && 'bg-primary/70',
      )}
    />
  );
}
