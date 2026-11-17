import { base } from '$app/paths';
import { appPath } from '$lib/navigation';
import { api, type TagType } from '$lib/api';
import { settings } from './settings.svelte';

export type Chip = { tag: string; type: TagType | null };
export type TagState = 'include' | 'exclude' | null;

export interface TagEditor {
    typed: string;
    chips: Chip[];
    add(tag: string, type?: TagType | null): void;
    commitTyped(): void;
}

export interface TagTarget {
    state(name: string): TagState;
    add(tag: string, type?: TagType | null): void;
    remove(name: string): void;
    searchHref(name: string): string;
}

export const RATING_BUTTONS = ['general', 'sensitive', 'questionable', 'explicit'] as const;

export const bare = (tag: string) => tag.replace(/^-/, '');
export const normalizeTag = (tag: string) => tag.trim().toLowerCase().replace(/\s+/g, '_');
export const looseTag = (tag: string) => tag.trim().toLowerCase().replace(/[\s_-]+/g, '_');
export const splitQuery = (query: string) => query.split(/\s+/).filter(Boolean);

export function similarId(tag: string) {
    const match = bare(tag).match(/^similar:(\d+)$/);
    return match ? Number(match[1]) : null;
}

type SearchSlot = 'lastSearch' | 'likedSearch' | 'dislikedSearch';

const OWN_SEARCHES: Record<string, SearchSlot> = { '/likes': 'likedSearch', '/disliked': 'dislikedSearch' };

const slotFor = (path: string): SearchSlot => OWN_SEARCHES[path] ?? 'lastSearch';

export const sharesSearch = (path: string, other: string) => slotFor(path) === slotFor(other);

export function rememberedSearch(path: string) {
    return settings[slotFor(path)];
}

export function rememberSearch(path: string, query: string) {
    settings[slotFor(path)] = query;
}

export function urlQuery(url: URL) {
    return url.searchParams.get('q') ?? rememberedSearch(appPath(url));
}

export function searchHref(path: string, query: string) {
    return `${base}${path}?q=${encodeURIComponent(query)}`;
}

export function withTag(chips: Chip[], raw: string, type: TagType | null = null): Chip[] {
    const tag = normalizeTag(raw);
    if (!bare(tag)) return chips;
    return [...chips.filter((chip) => bare(chip.tag) !== bare(tag)), { tag, type }];
}

export function tagStateIn(tags: string[], name: string): TagState {
    const tag = tags.find((tag) => bare(tag) === name);
    return tag === undefined ? null : tag.startsWith('-') ? 'exclude' : 'include';
}

class StagedSearch implements TagTarget, TagEditor {
    chips = $state<Chip[]>([]);
    typed = $state('');
    tags = $derived(this.chips.map((chip) => chip.tag));
    query = $derived(this.tags.join(' '));
    pendingQuery = $derived([this.query, normalizeTag(this.typed)].filter(Boolean).join(' '));

    async load(query: string) {
        const tags = splitQuery(query);
        this.chips = tags.map((tag) => ({ tag, type: null }));
        this.typed = '';
        const found = await api.tagsNamed(tags.map(bare));
        const types = new Map(found.map((tag) => [tag.name, tag.type]));
        this.chips = this.chips.map((chip) => ({ ...chip, type: types.get(bare(chip.tag)) ?? chip.type }));
    }

    add(raw: string, type: TagType | null = null) {
        this.chips = withTag(this.chips, raw, type);
        this.typed = '';
    }

    remove(name: string) {
        this.chips = this.chips.filter((chip) => bare(chip.tag) !== name);
    }

    toggleExclude(chip: Chip) {
        chip.tag = chip.tag.startsWith('-') ? bare(chip.tag) : `-${chip.tag}`;
    }

    toggle(tag: string) {
        if (this.state(bare(tag))) this.remove(bare(tag));
        else this.add(tag);
    }

    state(name: string) {
        return tagStateIn(this.tags, name);
    }

    clear() {
        this.chips = [];
        this.typed = '';
    }

    commitTyped() {
        if (this.typed.trim()) this.add(this.typed);
    }

    ratingQuery(name: string) {
        return [...this.tags.filter((tag) => tag.startsWith('rating:')), name].join(' ');
    }

    searchHref(name: string) {
        return searchHref('/', this.ratingQuery(name));
    }
}

export const staged = new StagedSearch();
