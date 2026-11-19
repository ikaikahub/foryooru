import { api, type Post, type SavedFile } from '$lib/api';
import { toast } from '$lib/state/toast.svelte';
import { postUrl } from '$lib/sites';
import { appPath, hrefWith, runSearch } from '$lib/navigation';
import { signal } from '$lib/signals';
import { page } from '$app/state';
import { settings } from '$lib/state/settings.svelte';
import { rememberedSearch, sharesSearch, similarId, splitQuery, staged } from '$lib/state/search.svelte';

export const announce = (saved: SavedFile) => toast.show(`${saved.name} saved to ${saved.folder}`);

async function shared<T>(post: Post, action: Promise<T>) {
    const result = await action;
    signal(post.id, 'shared');
    return result;
}

export const openPostLink = async (post: Post) => window.open(postUrl(post.id));
export const copyPostLink = (post: Post) => shared(post, navigator.clipboard.writeText(postUrl(post.id)));

export function copyPageLink(post: Post) {
    const href = hrefWith(page.url, { site: settings.site, post: post.id });
    return shared(post, navigator.clipboard.writeText(new URL(href, page.url).href));
}
export const copyImageLink = (post: Post) => shared(post, navigator.clipboard.writeText(post.fileUrl || postUrl(post.id)));
export const copyImage = (post: Post) => shared(post, api.copyImage(post.id));

export async function saveSample(post: Post) {
    announce(await shared(post, api.saveSample(post.id)));
}

export async function saveOriginal(post: Post) {
    announce(await shared(post, api.saveOriginal(post.id)));
}

export function searchSimilar(post: Post) {
    signal(post.id, 'similar');
    const browsing = sharesSearch(appPath(), '/');
    const tags = browsing ? staged.tags : splitQuery(rememberedSearch('/'));
    const query = [...tags.filter((tag) => similarId(tag) === null), `similar:${post.id}`].join(' ');
    return browsing ? runSearch(query) : runSearch(query, '/');
}
