import { api, errorMessage, type Post, type PostState, type SavedSearch, type Tag, type TagList } from '$lib/api';
import { page } from '$app/state';
import { SvelteMap } from 'svelte/reactivity';
import { splitQuery, urlQuery } from './search.svelte';
import { currentSite } from '$lib/sites';
import { settings } from './settings.svelte';
import { toast } from './toast.svelte';

class Library {
    favouriteTags = $state<Tag[]>([]);
    blacklist = $state<Tag[]>([]);
    savedSearches = $state<SavedSearch[]>([]);
    reactions = new SvelteMap<number, PostState>();

    starred = $derived(new Set(this.favouriteTags.map((tag) => tag.name)));
    blacklisted = $derived(new Set(this.blacklist.map((tag) => tag.name)));
    savedQueries = $derived(new Set(this.savedSearches.map((search) => search.query)));

    async load() {
        [this.favouriteTags, this.blacklist, this.savedSearches] = await Promise.all([
            api.tagList('favouriteTags'),
            api.tagList('blacklist'),
            api.savedSearches(),
        ]);
    }

    private setList(list: TagList, tags: Tag[]) {
        if (list === 'favouriteTags') this.favouriteTags = tags;
        else this.blacklist = tags;
    }

    async toggleTag(list: TagList, name: string) {
        this.setList(list, await api.toggleTag(list, name));
    }

    async addTags(list: TagList, names: string) {
        this.setList(list, await api.addTags(list, names));
    }

    async toggleSavedSearch(query: string) {
        this.savedSearches = await api.toggleSavedSearch(query);
    }

    private posts = new Map<number, Post>();
    private toggling = new Map<number, number>();
    private versions = new Map<number, number>();

    remember(posts: Post[]) {
        for (const post of posts) {
            this.posts.set(post.id, post);
            if (!this.toggling.has(post.id)) {
                this.reactions.set(post.id, { id: post.id, liked: post.liked, disliked: post.disliked });
            }
        }
    }

    post(id: number) {
        return this.posts.get(id) ?? null;
    }

    reaction(id: number) {
        return this.reactions.get(id) ?? { id, liked: false, disliked: false };
    }

    async toggleLike(id: number) {
        const before = this.reaction(id);
        const version = (this.versions.get(id) ?? 0) + 1;
        this.versions.set(id, version);
        this.toggling.set(id, (this.toggling.get(id) ?? 0) + 1);
        this.reactions.set(id, { id, liked: !before.liked, disliked: false });
        try {
            const state = await api.toggleLike(id, splitQuery(urlQuery(page.url)));
            if (this.versions.get(id) !== version) return;
            this.reactions.set(id, state);
            // likes also become site favourites whenever the login for that is filled in
            const canSync = settings.site === 'gelbooru' ? !!settings.gelbooruSession : !!(settings.danbooruLogin && settings.danbooruApiKey);
            if (canSync) {
                api.setSiteFavourite(id, state.liked).catch((error) =>
                    toast.show(`Couldn't update your ${currentSite().label} favourites: ${errorMessage(error)}`),
                );
            }
        } catch (error) {
            if (this.versions.get(id) !== version) return;
            this.reactions.set(id, before);
            toast.show(errorMessage(error));
        } finally {
            const remaining = (this.toggling.get(id) ?? 1) - 1;
            if (remaining > 0) this.toggling.set(id, remaining);
            else this.toggling.delete(id);
        }
    }

    async like(id: number) {
        if (!this.reaction(id).liked) await this.toggleLike(id);
    }

    async toggleDislike(id: number) {
        const before = this.reaction(id);
        const version = (this.versions.get(id) ?? 0) + 1;
        this.versions.set(id, version);
        this.toggling.set(id, (this.toggling.get(id) ?? 0) + 1);
        this.reactions.set(id, { id, liked: false, disliked: !before.disliked });
        try {
            const state = await api.toggleDislike(id);
            if (this.versions.get(id) !== version) return;
            this.reactions.set(id, state);
        } catch (error) {
            if (this.versions.get(id) !== version) return;
            this.reactions.set(id, before);
            toast.show(errorMessage(error));
        } finally {
            const remaining = (this.toggling.get(id) ?? 1) - 1;
            if (remaining > 0) this.toggling.set(id, remaining);
            else this.toggling.delete(id);
        }
    }

    async clearLikes() {
        await api.clearLikes();
        for (const [id, reaction] of this.reactions) if (reaction.liked) this.reactions.set(id, { ...reaction, liked: false });
    }
}

export const library = new Library();
