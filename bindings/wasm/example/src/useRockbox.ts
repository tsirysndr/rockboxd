import { useCallback, useRef, useState } from "react";
import {
  RockboxPlayer,
  type ProgressEvent,
  type StatusEvent,
  type TrackEvent,
} from "rockbox-wasm";

const INITIAL_STATUS: StatusEvent = {
  state: "stopped",
  index: -1,
  queue_len: 0,
  shuffle: false,
  repeat: 0,
};

/**
 * Wraps a single RockboxPlayer and mirrors its events into React state.
 * The player is created lazily and booted on the first user gesture
 * (`ensureReady`) — browsers won't start an AudioContext otherwise.
 */
export function useRockbox() {
  const ref = useRef<RockboxPlayer | null>(null);
  const [ready, setReady] = useState(false);
  const [status, setStatus] = useState<StatusEvent>(INITIAL_STATUS);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [track, setTrack] = useState<TrackEvent | null>(null);
  const [error, setError] = useState<string | null>(null);

  const getPlayer = useCallback(() => {
    if (!ref.current) {
      // The dist files are served from /rockbox (see scripts/copy-wasm.mjs).
      const p = new RockboxPlayer({ baseUrl: "/rockbox" });
      p.on("status", setStatus);
      p.on("progress", setProgress);
      p.on("track", setTrack);
      p.on("error", (e) => setError(e.message));
      ref.current = p;
    }
    return ref.current;
  }, []);

  const ensureReady = useCallback(async () => {
    const p = getPlayer();
    if (!p.ready) {
      setError(null);
      try {
        await p.init();
        setReady(true);
      } catch (e) {
        setError((e as Error).message);
        throw e;
      }
    }
    return p;
  }, [getPlayer]);

  return { getPlayer, ensureReady, ready, status, progress, track, error };
}
