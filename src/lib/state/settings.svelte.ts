import type { GelbooruSession, SaveFolder } from '$lib/api';
import type { Theme } from '$lib/theme';
import { persisted } from './persisted.svelte';

export const MAX_COLUMNS = 10;
export const DEFAULT_SIDEBAR_WIDTH = 300;

const saved = persisted('settings.json', {
    theme: 'system' as Theme,
    apiKey: '',
    userId: '',
    danbooruLogin: '',
    danbooruApiKey: '',
    gelbooruSession: null as GelbooruSession | null,
    forYouDiscover: false,
    feedDiscover: false,
    hideLiked: false,
    similarPerArtist: 6 as number | null,
    lastSearch: 'rating:general',
    likedSearch: '',
    dislikedSearch: '',
    sidebarWidth: DEFAULT_SIDEBAR_WIDTH,
    sidebarCollapsed: false,
    columns: 0,
    gridAutoplay: false,
    saveFolder: null as SaveFolder | null,
    debug: false,
});

export const settings = saved.state;

export async function restoreSettings() {
    await saved.restore();
}
