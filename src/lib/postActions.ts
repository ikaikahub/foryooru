import { api, type Post, type SavedFile, type Site } from '$lib/api';
import { toast } from '$lib/state/toast.svelte';
import { SITES } from '$lib/sites';
import { appPath, hrefWith, runSearch } from '$lib/navigation';
import { signal } from '$lib/signals';
import { page } from '$app/state';
import { settings } from '$lib/state/settings.svelte';
import { rememberedSearch, sharesSearch, similarId, splitQuery, staged } from '$lib/state/search.svelte';

export const announce = (saved: SavedFile) => toast.show(`${saved.name} saved to ${saved.folder}`);

// a post from the other site has no row here, so its signals would land on whatever post has that id
async function shared<T>(post: Post, site: Site, action: Promise<T>) {
    const result = await action;
    if (site === settings.site) signal(post.id, 'shared');
    return result;
}

export const openPostLink = async (post: Post, site: Site) => window.open(SITES[site].postUrl(post.id));
export const copyPostLink = (post: Post, site: Site) => shared(post, site, navigator.clipboard.writeText(SITES[site].postUrl(post.id)));

export function copyPageLink(post: Post, site: Site) {
    const href = hrefWith(page.url, { site, post: post.id });
    return shared(post, site, navigator.clipboard.writeText(new URL(href, page.url).href));
}
export const copyImageLink = (post: Post, site: Site) =>
    shared(post, site, navigator.clipboard.writeText(post.fileUrl || SITES[site].postUrl(post.id)));
export const copyImage = (post: Post, site: Site) => shared(post, site, api.copyImage(post.id, site));

export async function saveSample(post: Post, site: Site) {
    announce(await shared(post, site, api.saveSample(post.id, site)));
}

export async function saveOriginal(post: Post, site: Site) {
    announce(await shared(post, site, api.saveOriginal(post.id, site)));
}

export function searchSimilar(post: Post) {
    signal(post.id, 'similar');
    const browsing = sharesSearch(appPath(), '/');
    const tags = browsing ? staged.tags : splitQuery(rememberedSearch('/'));
    const query = [...tags.filter((tag) => similarId(tag) === null), `similar:${post.id}`].join(' ');
    return browsing ? runSearch(query) : runSearch(query, '/');
}
