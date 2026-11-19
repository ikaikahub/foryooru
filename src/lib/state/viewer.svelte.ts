import { goto, pushState, replaceState } from '$app/navigation';
import { page } from '$app/state';
import { api, errorMessage, type Site } from '$lib/api';
import { addImpression } from '$lib/impressions';
import { goBack, hrefWith } from '$lib/navigation';
import { library } from '$lib/state/library.svelte';
import { settings } from '$lib/state/settings.svelte';
import { toast } from '$lib/state/toast.svelte';
import { SITES } from '$lib/sites';
import { tick, untrack } from 'svelte';

export type PageTurner = {
    current: number;
    pages: number;
    href: (page: number) => string;
    load: (page: number) => Promise<number[]>;
};

// links carry the site and the post so they reopen the same image
const postHref = (id: number) => hrefWith(page.url, { site: settings.site, post: id });

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
        pushState(postHref(id), { post: id, ids, depth: 1, listPage: turner?.current });
    }

    // a shared link opens its post without adding a history entry, there is nothing behind it
    openLinked(id: number) {
        this.turner = null;
        this.turnError = null;
        this.opening = id;
        replaceState('', { post: id, ids: [id], depth: 1, fromUrl: true });
    }

    show(id: number, ids: number[]) {
        if (this.current === null) {
            this.open(id, ids);
            return;
        }
        this.opening = id;
        pushState(postHref(id), { ...page.state, post: id, ids, listPage: undefined, depth: (page.state.depth ?? 1) + 1 });
    }

    step(delta: number) {
        if (this.closing) return;
        const next = this.ids[this.index + delta];
        if (next === undefined) {
            this.turnPage(delta);
            return;
        }
        pushState(postHref(next), { ...page.state, post: next, depth: (page.state.depth ?? 1) + 1 });
    }

    async close() {
        if (this.closing) return;
        const { depth = 1, turned = false, fromUrl = false } = page.state;
        this.closing = true;
        try {
            if (fromUrl) {
                // there is no list behind a shared link, so just drop the post from the address
                const target = new URL(page.url);
                target.searchParams.delete('post');
                target.searchParams.delete('site');
                replaceState(target.pathname + target.search, {});
                return;
            }
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
            const href = hrefWith(new URL(turner.href(target), page.url), { site: settings.site, post });
            await goto(href, { state: { post, ids, depth: 1, turned: true, listPage: target } });
        } catch (error) {
            this.turnError = error;
        } finally {
            this.turning = false;
        }
    }
}

// called once the app is running, when the address came from a shared link
export async function openLinkedPost() {
    const id = Number(page.url.searchParams.get('post'));
    if (!id || page.state.post === id) return;
    const linkedSite = page.url.searchParams.get('site');
    if (linkedSite !== null && linkedSite !== settings.site) {
        const label = SITES[linkedSite as Site]?.label ?? linkedSite;
        toast.show(`This link is for ${label}. Switch sites to open it.`);
        return;
    }
    try {
        const post = await api.openPost(id);
        library.remember([post]);
        viewer.openLinked(post.id);
    } catch (error) {
        toast.show(errorMessage(error));
    }
}

export const viewer = new Viewer();
