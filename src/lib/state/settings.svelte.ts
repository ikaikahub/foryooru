import type { GelbooruSession, SaveFolder, Site } from '$lib/api';
import type { Theme } from '$lib/theme';
import { sendSettings } from '$lib/engine/client';
import { persisted } from './persisted.svelte';
import { GELBOORU_API } from '$lib/sites';

export const MAX_COLUMNS = 10;
export const DEFAULT_SIDEBAR_WIDTH = 300;

const saved = persisted('settings.json', {
    theme: 'system' as Theme,
    site: 'danbooru' as Site,
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
});

export const settings = saved.state;

const RETIRED_SITE = 'kimochii';

export async function restoreSettings() {
    await saved.restore();
    if ((settings.site as string) === RETIRED_SITE) settings.site = 'gelbooru';
    // all tabs share one engine, which serves one site at a time, so a switch in another tab moves this one too
    const site = settings.site;
    addEventListener('storage', (event) => {
        if (event.key === 'settings.json' && settings.site !== site) location.reload();
    });
}

export const settingsForEngine = (): Record<string, unknown> => ({ ...$state.snapshot(settings), gelbooruApi: GELBOORU_API });

// the engine reads account keys and the site from its own copy, refreshed on every change
export function syncSettingsToEngine() {
    $effect.root(() => {
        $effect(() => sendSettings(settingsForEngine()));
    });
}
