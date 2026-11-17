import type { Site } from '$lib/api';
import { settings } from '$lib/state/settings.svelte';

type SiteInfo = { label: string; postUrl: (id: number) => string };

export const SITES: Record<Site, SiteInfo> = {
    gelbooru: { label: 'Gelbooru', postUrl: (id) => `https://gelbooru.com/index.php?page=post&s=view&id=${id}` },
    danbooru: { label: 'Danbooru', postUrl: (id) => `https://danbooru.donmai.us/posts/${id}` },
};

// browsers can't reach Gelbooru directly: its API goes through a Vercel function (relay/ in the repo)
// and its images through a Cloudflare Worker (proxy/), since Gelbooru blocks the Worker's API calls
export const GELBOORU_API = 'https://foryooru-relay.vercel.app/api/gelbooru';
export const GELBOORU_IMAGES = 'https://foryooru-proxy.foryooru.workers.dev';

export const currentSite = () => SITES[settings.site];

export const postUrl = (id: number) => currentSite().postUrl(id);
