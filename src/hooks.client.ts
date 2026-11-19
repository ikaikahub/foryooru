import { startEngine } from '$lib/engine/client';
import { restoreSettings, settingsForEngine, syncSettingsToEngine } from '$lib/state/settings.svelte';
import type { ClientInit } from '@sveltejs/kit';

// settings come first: the engine needs the site and the account keys to start
export const init: ClientInit = async () => {
    await restoreSettings();
    // the database lives in the origin private file system, which older browsers don't provide at all
    if (!navigator.storage?.getDirectory) {
        showSplashMessage("This browser can't store Foryooru's database. Try a current Chrome, Edge, Firefox or Safari.");
        return new Promise<never>(() => {});
    }
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
    if (message.includes('SecurityError')) {
        // private windows refuse the storage the database lives in, and their error names mean nothing to a reader
        showSplashMessage('Foryooru needs local storage, which private windows block. Open it in a normal window.');
    } else if (message.includes('NoModificationAllowedError')) {
        showSplashMessage('Foryooru is open in another tab. Close it, then reload this one.');
    } else {
        showSplashMessage(`Foryooru couldn't start: ${message}`);
    }
}

function showSplashMessage(message: string) {
    const label = document.querySelector('#splash > span');
    if (!label) return;
    label.textContent = message;
    document.getElementById('splash')?.classList.add('failed');
}
