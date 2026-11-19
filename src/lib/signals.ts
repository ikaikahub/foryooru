import { api, type Signal } from '$lib/api';

const FLUSH_EVERY_MS = 5_000;
const FLUSH_AT = 10;

type SignalEntry = { post: number; signal: Signal; tag: string | null };

let queue: SignalEntry[] = [];

export async function flushSignals() {
    if (!queue.length) return;
    const signals = queue;
    queue = [];
    await api.recordSignals(signals).catch((error) => console.error('Recording signals failed', error));
}

export function signal(post: number, kind: Signal, tag?: string) {
    queue.push({ post, signal: kind, tag: tag ?? null });
    if (queue.length >= FLUSH_AT) void flushSignals();
}

setInterval(flushSignals, FLUSH_EVERY_MS);
document.addEventListener('visibilitychange', () => {
    if (document.hidden) void flushSignals();
});
