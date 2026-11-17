import { api, type Post, type SavedFile } from '$lib/api';
import { toast } from '$lib/state/toast.svelte';
import { CANCELLED } from '$lib/components/ActionButton.svelte';
import { postUrl } from '$lib/sites';
import { appPath, runSearch } from '$lib/navigation';
import { signal } from '$lib/signals';
import { rememberedSearch, sharesSearch, similarId, splitQuery, staged } from '$lib/state/search.svelte';

export const announce = (saved: SavedFile) => toast.show(`${saved.name} saved to ${saved.folder}`);

async function shared<T>(post: Post, action: Promise<T>) {
    const result = await action;
    signal(post.id, 'shared');
    return result;
}

export const openPostLink = async (post: Post) => window.open(postUrl(post.id));
export const copyPostLink = (post: Post) => shared(post, navigator.clipboard.writeText(postUrl(post.id)));
export const copyImageLink = (post: Post) => shared(post, navigator.clipboard.writeText(post.fileUrl || postUrl(post.id)));
export const copyImage = (post: Post) => shared(post, api.copyImage(post.id));

export async function saveOriginal(post: Post) {
    announce(await shared(post, api.saveOriginal(post.id)));
}

export async function saveOriginalAs(post: Post) {
    const saved = await api.saveOriginalAs(post.id);
    if (!saved) return CANCELLED;
    signal(post.id, 'shared');
    announce(saved);
}

export function searchSimilar(post: Post) {
    signal(post.id, 'similar');
    const browsing = sharesSearch(appPath(), '/');
    const tags = browsing ? staged.tags : splitQuery(rememberedSearch('/'));
    const query = [...tags.filter((tag) => similarId(tag) === null), `similar:${post.id}`].join(' ');
    return browsing ? runSearch(query) : runSearch(query, '/');
}
