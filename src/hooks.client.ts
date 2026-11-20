import { api, loadVideoBase } from '$lib/api';
import { restoreSettings, settings } from '$lib/state/settings.svelte';
import type { ClientInit } from '@sveltejs/kit';

export const init: ClientInit = async () => {
    await api.waitUntilReady();
    await Promise.all([restoreSettings(), loadVideoBase()]);
    await api.setTimings(settings.debug);
};
