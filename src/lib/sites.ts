import type { Post, Site } from '$lib/api';

type SiteInfo = { label: string; postUrl: (id: number) => string };

export const SITES: Record<Site, SiteInfo> = {
    gelbooru: { label: 'Gelbooru', postUrl: (id) => `https://gelbooru.com/index.php?page=post&s=view&id=${id}` },
    danbooru: { label: 'Danbooru', postUrl: (id) => `https://danbooru.donmai.us/posts/${id}` },
};

// the post's page on Danbooru when it has one, else on Gelbooru
export function postUrl(post: Post) {
    const { danbooru, gelbooru } = post.sites;
    if (danbooru !== null) return SITES.danbooru.postUrl(danbooru);
    return gelbooru === null ? null : SITES.gelbooru.postUrl(gelbooru);
}
