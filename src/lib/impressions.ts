import { api, type Impression } from '$lib/api';

const FLUSH_EVERY_MS = 15_000;
const FLUSH_AT = 15;

let queue: Impression[] = [];

export async function flushImpressions() {
    if (!queue.length) return;
    const impressions = queue;
    queue = [];
    await api.recordImpressions(impressions).catch((error) => console.error('Recording impressions failed', error));
}

export function addImpression(impression: Impression) {
    queue.push(impression);
    if (queue.length >= FLUSH_AT) flushImpressions();
}

setInterval(flushImpressions, FLUSH_EVERY_MS);
document.addEventListener('visibilitychange', () => {
    if (document.hidden) flushImpressions();
});
