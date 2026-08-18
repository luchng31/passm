import { useCallback, useEffect, useRef, useState } from 'react';
import type { CopyField } from './api';

export type { CopyField };

export interface CopyTimerState {
  /** Field of the entry currently showing "已复制" feedback, or null. */
  copiedField: CopyField | null;
  /** Id of the entry currently showing copy feedback, or null. */
  copiedId: string | null;
  /** Marks a field as copied and clears the feedback after `clearMs`. */
  copyWithTimer: (field: CopyField, id: string) => void;
}

/**
 * Copy feedback timer: shows "已复制" on the button that was clicked and
 * clears it after `clearMs`. Re-copying restarts the timer; the timer is
 * cleared on unmount.
 */
export function useCopyTimer(clearMs = 2000): CopyTimerState {
  const [copied, setCopied] = useState<{ field: CopyField; id: string } | null>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
      }
    };
  }, []);

  const copyWithTimer = useCallback(
    (field: CopyField, id: string) => {
      setCopied({ field, id });
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
      }
      timerRef.current = window.setTimeout(() => setCopied(null), clearMs);
    },
    [clearMs]
  );

  return {
    copiedField: copied?.field ?? null,
    copiedId: copied?.id ?? null,
    copyWithTimer,
  };
}