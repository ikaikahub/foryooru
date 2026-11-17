import type { TagType } from '$lib/api';
import type { TagTarget } from './search.svelte';

export type TagAction = { label: string; action: () => unknown };

type OpenMenu = { name: string; type: TagType | null; anchor: HTMLElement; target: TagTarget; actions: TagAction[] };

class TagMenu {
    current = $state<OpenMenu | null>(null);

    show(anchor: HTMLElement, name: string, type: TagType | null, target: TagTarget, actions: TagAction[] = []) {
        this.current = { anchor, name, type, target, actions };
    }

    close() {
        this.current = null;
    }
}

export const tagMenu = new TagMenu();
