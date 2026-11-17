import { goto, pushState, replaceState } from '$app/navigation';
import { page } from '$app/state';
import { addImpression } from '$lib/impressions';
import { goBack } from '$lib/navigation';
import { tick, untrack } from 'svelte';

export type PageTurner = {
    current: number;
    pages: number;
    href: (page: number) => string;
    load: (page: number) => Promise<number[]>;
};

class Viewer {
    current = $derived(page.state.post ?? null);
    ids = $derived(page.state.ids ?? []);
    index = $derived(this.current === null ? -1 : this.ids.indexOf(this.current));
    closing = $state(false);
    shown = $derived(this.current !== null && !this.closing);
    turnError = $state<unknown>(null);
    private turner: PageTurner | null = null;
    private turning = false;
    private opening: number | null = null;
    private showing: { post: number; since: number; opened: boolean } | null = null;
    private hiddenAt: number | null = null;

    constructor() {
        document.addEventListener('visibilitychange', () => {
            if (document.hidden) this.hiddenAt = performance.now();
            else if (this.hiddenAt !== null) {
                if (this.showing) this.showing.since += performance.now() - this.hiddenAt;
                this.hiddenAt = null;
            }
        });
        $effect.root(() => {
            $effect(() => {
                const id = this.current;
                untrack(() => this.track(id));
            });
        });
    }

    private track(id: number | null) {
        if (this.showing?.post === id) return;
        if (this.showing) {
            const { post, since, opened } = this.showing;
            addImpression({ post, source: 'viewer', ms: Math.round(performance.now() - since), opened });
        }
        this.showing = id === null ? null : { post: id, since: performance.now(), opened: this.opening === id };
        this.opening = null;
    }

    open(id: number, ids: number[], turner: PageTurner | null = null) {
        this.turner = turner;
        this.turnError = null;
        this.opening = id;
        pushState('', { post: id, ids, depth: 1, listPage: turner?.current });
    }

    show(id: number, ids: number[]) {
        if (this.current === null) {
            this.open(id, ids);
            return;
        }
        this.opening = id;
        pushState('', { ...page.state, post: id, ids, listPage: undefined, depth: (page.state.depth ?? 1) + 1 });
    }

    step(delta: number) {
        if (this.closing) return;
        const next = this.ids[this.index + delta];
        if (next === undefined) {
            this.turnPage(delta);
            return;
        }
        pushState('', { ...page.state, post: next, depth: (page.state.depth ?? 1) + 1 });
    }

    async close() {
        if (this.closing) return;
        const { depth = 1, turned = false } = page.state;
        this.closing = true;
        try {
            const steps = turned ? depth - 1 : depth;
            if (steps > 0) {
                await goBack(steps);
                await tick();
            }
            if (turned) replaceState('', {});
        } finally {
            this.closing = false;
        }
    }

    private async turnPage(delta: number) {
        const turner = this.turner;
        const listPage = page.state.listPage;
        if (!turner || listPage === undefined || this.turning) return;
        const target = listPage + delta;
        if (target < 1 || target > turner.pages) return;
        this.turning = true;
        this.turnError = null;
        try {
            const ids = await turner.load(target);
            const post = delta > 0 ? ids[0] : ids[ids.length - 1];
            if (post === undefined) return;
            await goto(turner.href(target), { state: { post, ids, depth: 1, turned: true, listPage: target } });
        } catch (error) {
            this.turnError = error;
        } finally {
            this.turning = false;
        }
    }
}

export const viewer = new Viewer();
