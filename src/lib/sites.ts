import type { Site } from '$lib/api';
import { settings } from '$lib/state/settings.svelte';

type SiteInfo = { label: string; postUrl: (id: number) => string };

export const SITES: Record<Site, SiteInfo> = {
    gelbooru: { label: 'Gelbooru', postUrl: (id) => `https://gelbooru.com/index.php?page=post&s=view&id=${id}` },
    danbooru: { label: 'Danbooru', postUrl: (id) => `https://danbooru.donmai.us/posts/${id}` },
};

export const currentSite = () => SITES[settings.site];

export const postUrl = (id: number) => currentSite().postUrl(id);
