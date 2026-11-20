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
    // ?reset is the way out when a damaged database stops the engine from starting
    const url = new URL(location.href);
    if (url.searchParams.has('reset')) {
        url.searchParams.delete('reset');
        await clearStorage().catch((error) => showSplashMessage(`Couldn't clear the database: ${error}`));
        location.replace(url.href);
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

// the origin private file system only holds the databases, so emptying it resets everything stored locally
async function clearStorage() {
    const root = await navigator.storage.getDirectory();
    for await (const [name] of root.entries()) {
        await root.removeEntry(name, { recursive: true });
    }
}

function showStartError(error: unknown) {
    const message = String((error as { message?: string })?.message ?? error);
    if (message.includes('SecurityError')) {
        // private windows refuse the storage the database lives in, and their error names mean nothing to a reader
        showSplashMessage('Foryooru needs local storage, which private windows block. Open it in a normal window.');
    } else if (message.includes('NoModificationAllowedError')) {
        showSplashMessage('Foryooru is open in another tab. Close it, then reload this one.');
    } else if (/malformed|not a database|corrupt/i.test(message)) {
        showSplashMessage(`Foryooru couldn't start: ${message}`, [
            { label: 'Save database', run: saveDatabases },
            {
                label: 'Reset database',
                run: () => {
                    const warning = 'Delete the local database and start over? Everything stored for this site will be lost.';
                    if (confirm(warning)) clearStorage().then(() => location.reload());
                },
            },
        ]);
    } else {
        showSplashMessage(`Foryooru couldn't start: ${message}`);
    }
}

// hands the damaged files over for diagnosis; they live inside the browser's storage otherwise
async function saveDatabases() {
    const root = await navigator.storage.getDirectory();
    const files: File[] = [];
    const collect = async (dir: FileSystemDirectoryHandle): Promise<void> => {
        for await (const [, handle] of dir.entries()) {
            if (handle.kind === 'directory') await collect(handle);
            else files.push(await handle.getFile());
        }
    };
    await collect(root);
    if (files.length === 0) {
        alert('There are no database files left to save.');
        return;
    }
    for (const file of files) {
        const link = document.createElement('a');
        link.href = URL.createObjectURL(file);
        link.download = file.name;
        document.body.append(link);
        link.click();
        link.remove();
        setTimeout(() => URL.revokeObjectURL(link.href), 60_000);
    }
}

function showSplashMessage(message: string, actions: { label: string; run: () => void }[] = []) {
    const label = document.querySelector('#splash > span');
    if (!label) return;
    label.textContent = message;
    document.getElementById('splash')?.classList.add('failed');
    for (const action of actions) {
        const button = document.createElement('button');
        button.textContent = action.label;
        button.style.cssText =
            'margin-top: 0.5rem; padding: 0.5rem 1rem; border: 1px solid currentColor; border-radius: 0.6rem; background: transparent; color: inherit; font: inherit; cursor: pointer';
        button.onclick = action.run;
        label.after(button);
    }
}
