import { base } from '$app/paths';
import { MAGIC_WAND } from '$lib/emoji';
import { appPath, isCurrentPage } from '$lib/navigation';
import { currentSite } from '$lib/sites';
import { requestRerun } from '$lib/state/rerun.svelte';
import { rememberedSearch, searchHref, sharesSearch, staged } from '$lib/state/search.svelte';

export type Tab = { path: string; label: string; title: () => string; searches: boolean } & ({ emoji: string } | { image: string });

export const TABS: Tab[] = [
    { path: '/', emoji: '🔍', label: 'Browse', title: () => `Search ${currentSite().label}`, searches: true },
    { path: '/for-you', image: MAGIC_WAND, label: 'For you', title: () => 'Recommendations', searches: true },
    { path: '/tiktok', emoji: '📱', label: 'TikTok', title: () => 'TikTok mode', searches: true },
    { path: '/settings', emoji: '⚙️', label: 'Settings', title: () => 'Settings', searches: false },
    { path: '/info', emoji: 'ℹ️', label: 'Info', title: () => 'About and keybinds', searches: false },
];

export function tabHref(tab: Tab) {
    if (!tab.searches) return base + tab.path;
    const query = sharesSearch(appPath(), tab.path) ? staged.pendingQuery : rememberedSearch(tab.path);
    return searchHref(tab.path, query);
}

export function rerunIfCurrent(e: MouseEvent, tab: Tab, href: string) {
    if (!tab.searches || !isCurrentPage(href)) return;
    e.preventDefault();
    requestRerun();
}
