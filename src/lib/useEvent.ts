import { useEffect, useRef } from "react";
import { on } from "./api";

/**
 * Subscribes to a backend event for the lifetime of the component.
 * Safe with React StrictMode: if the component unmounts before the subscription resolves,
 * the listener is removed as soon as it exists (otherwise every message arrived twice).
 */
export function useEvent<T>(event: string, handler: (payload: T) => void) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void on<T>(event, (p) => ref.current(p)).then((f) => {
      if (cancelled) f();
      else off = f;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [event]);
}
