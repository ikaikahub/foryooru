import { startEngine } from '$lib/engine/client';
import { restoreSettings, settingsForEngine, syncSettingsToEngine } from '$lib/state/settings.svelte';
import type { ClientInit } from '@sveltejs/kit';

// settings come first: the engine needs the site and the account keys to start
export const init: ClientInit = async () => {
    await restoreSettings();
    await startEngine(settingsForEngine().site as string, settingsForEngine());
    syncSettingsToEngine();
};
