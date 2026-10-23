import { type EntityDetail, useGetEntity } from "@umbra/data";
import { useControlSeq, useStreamStatus } from "@umbra/engine-stream";
import { useEffect, useRef } from "react";

/** While the entity is in a running simulation the detail is read again this often (ASM-806). */
export const DETAIL_POLL_MS = 1000;

export interface EntityDetailState {
  readonly detail: EntityDetail | undefined;
  readonly failed: boolean;
}

/**
 * The REST detail of an entity: fetched when selected, polled every second while `polling`, read again at once
 * after every control frame (`controlSeq`, ASM-938) and once when the stream is back after a reconnect (frames
 * and control counts missed in the gap), never on tick frames. TanStack Query's own timers do the polling.
 */
export function useEntityDetail(id: number | null, polling: boolean): EntityDetailState {
  const query = useGetEntity(id ?? 0, {
    query: { enabled: id !== null, refetchInterval: polling ? DETAIL_POLL_MS : false },
  });
  const seq = useControlSeq();
  const status = useStreamStatus();
  const seenSeq = useRef(seq);
  const seenStatus = useRef(status);
  const { refetch } = query;
  useEffect(() => {
    if (seenSeq.current === seq) return;
    seenSeq.current = seq;
    if (id !== null) void refetch();
  }, [seq, id, refetch]);
  useEffect(() => {
    const before = seenStatus.current;
    seenStatus.current = status;
    if (before === "reconnecting" && status === "open" && id !== null) void refetch();
  }, [status, id, refetch]);
  return { detail: id === null ? undefined : query.data, failed: id !== null && query.isError };
}
