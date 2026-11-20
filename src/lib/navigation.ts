import { goto, pushState } from '$app/navigation';
import { base } from '$app/paths';
import { page } from '$app/state';
import { api } from '$lib/api';
import { requestRerun } from '$lib/state/rerun.svelte';
import { urlQuery } from '$lib/state/search.svelte';

const SEARCHABLE_PAGES = new Set(['/', '/for-you', '/tiktok', '/following', '/likes', '/disliked']);

// the app's own paths leave out the base the site is served under (/foryooru on GitHub Pages)
export function appPath(url: URL = page.url) {
    return url.pathname.slice(base.length) || '/';
}

export function runSearch(query: string, path = SEARCHABLE_PAGES.has(appPath()) ? appPath() : '/') {
    api.recordSearch(query).catch((error) => console.error('Recording the search failed', error));
    const target = path === appPath() ? new URL(page.url) : new URL(base + path, page.url);
    target.searchParams.set('q', query);
    target.searchParams.delete('page');
    // the viewer's post params must not follow a new search, or a reload would reopen that post
    target.searchParams.delete('post');
    target.searchParams.delete('site');
    return openOrRerun(target.pathname + target.search);
}

export function isCurrentPage(href: string) {
    const target = new URL(href, page.url);
    return (
        target.pathname === page.url.pathname &&
        urlQuery(target) === urlQuery(page.url) &&
        pageNumber(target) === pageNumber(page.url)
    );
}

export function openOrRerun(href: string) {
    if (isCurrentPage(href)) requestRerun();
    else return goto(href);
}

export function goBack(steps: number) {
    return new Promise<void>((resolve) => {
        addEventListener('popstate', () => resolve(), { once: true });
        history.go(-steps);
    });
}

export function openLayer(layer: 'searching' | 'menuOpen') {
    if (!page.state[layer]) pushState('', { ...page.state, [layer]: true });
}

export function closeLayer(layer: 'searching' | 'menuOpen') {
    return page.state[layer] ? goBack(1) : Promise.resolve();
}

export function pageNumber(url: URL) {
    return Math.max(1, Number(url.searchParams.get('page')) || 1);
}

export function hrefWith(url: URL, params: Record<string, string | number>) {
    const target = new URL(url);
    for (const [key, value] of Object.entries(params)) target.searchParams.set(key, String(value));
    return target.pathname + target.search;
}
