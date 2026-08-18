import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useCopyTimer } from './copy';

describe('useCopyTimer', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows the copied field immediately and clears it after the timeout', () => {
    const { result } = renderHook(() => useCopyTimer(2000));

    act(() => {
      result.current.copyWithTimer('password', 'id-1');
    });
    expect(result.current.copiedField).toBe('password');
    expect(result.current.copiedId).toBe('id-1');

    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(result.current.copiedField).toBeNull();
    expect(result.current.copiedId).toBeNull();
  });

  it('restarts the timer when copying again before it expires', () => {
    const { result } = renderHook(() => useCopyTimer(2000));

    act(() => {
      result.current.copyWithTimer('username', 'id-1');
    });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    act(() => {
      result.current.copyWithTimer('url', 'id-2');
    });
    expect(result.current.copiedField).toBe('url');

    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current.copiedField).toBe('url');

    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current.copiedField).toBeNull();
  });
});