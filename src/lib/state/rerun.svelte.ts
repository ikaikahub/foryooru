import { untrack } from 'svelte';

const requests = $state({ count: 0 });

export function requestRerun() {
    requests.count++;
}

export function onRerun(callback: () => void) {
    let handled = untrack(() => requests.count);
    $effect(() => {
        const count = requests.count;
        if (count === handled) return;
        handled = count;
        untrack(callback);
    });
}
