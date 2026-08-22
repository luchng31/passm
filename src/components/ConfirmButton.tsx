import { useState } from 'react';

interface ConfirmButtonProps {
  label: string;
  confirmLabel?: string;
  cancelLabel?: string;
  className?: string;
  onConfirm: () => void;
}

/**
 * Two-step destructive action: first click arms the confirm state, second
 * click (确认) fires `onConfirm`. Inline and testable — no window.confirm.
 */
export function ConfirmButton({
  label,
  confirmLabel = '确认',
  cancelLabel = '取消',
  className = 'btn btn-danger btn-sm',
  onConfirm,
}: ConfirmButtonProps) {
  const [confirming, setConfirming] = useState(false);

  if (confirming) {
    return (
      <span className="confirm-inline">
        <button className="btn btn-danger btn-sm" onClick={onConfirm}>
          {confirmLabel}
        </button>
        <button className="btn btn-sm" onClick={() => setConfirming(false)}>
          {cancelLabel}
        </button>
      </span>
    );
  }

  return (
    <button className={className} onClick={() => setConfirming(true)}>
      {label}
    </button>
  );
}