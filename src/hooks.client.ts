import { startEngine } from '$lib/engine/client';
import { restoreSettings, settingsForEngine, syncSettingsToEngine } from '$lib/state/settings.svelte';
import type { ClientInit } from '@sveltejs/kit';

// settings come first: the engine needs the site and the account keys to start
export const init: ClientInit = async () => {
    await restoreSettings();
    try {
        await startEngine(settingsForEngine().site as string, settingsForEngine());
    } catch (error) {
        showStartError(error);
        // the app can't run without the engine, so it stays on the loading screen
        return new Promise<never>(() => {});
    }
    syncSettingsToEngine();
};

function showStartError(error: unknown) {
    const message = String((error as { message?: string })?.message ?? error);
    const label = document.querySelector('#splash span');
    if (!label) return;
    label.textContent = message.includes('NoModificationAllowedError')
        ? 'Foryooru is open in another tab. Close it, then reload this one.'
        : `Foryooru couldn't start: ${message}`;
    document.getElementById('splash')?.classList.add('failed');
}
