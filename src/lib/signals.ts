import { api, type Signal } from '$lib/api';

export function signal(post: number, kind: Signal, tag?: string) {
    api.recordSignal(post, kind, tag ?? null).catch((error) => console.error('Recording a signal failed', error));
}
