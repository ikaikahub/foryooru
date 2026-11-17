// each store is one JSON value in localStorage, under the file name the desktop app used for it
function read(file: string): Record<string, unknown> {
    try {
        return JSON.parse(localStorage.getItem(file) ?? '{}');
    } catch {
        return {};
    }
}

export function persisted<T extends object>(file: string, defaults: T) {
    const state = $state(structuredClone(defaults));

    async function restore() {
        Object.assign(state, read(file));
        // other tabs save their changes too, pick them up so this tab doesn't write its older copy back over them
        addEventListener('storage', (event) => {
            if (event.key === file) Object.assign(state, read(file));
        });
        $effect.root(() => {
            $effect(() => {
                const json = JSON.stringify($state.snapshot(state));
                try {
                    localStorage.setItem(file, json);
                } catch {
                    // private windows can refuse storage, the app still works for this visit
                }
            });
        });
    }

    return { state, restore };
}

export const snapshotPersisted = (file: string) => read(file);

// backups replace a whole store; the page reloads afterwards and picks the new values up
export function overwritePersisted(file: string, values: Record<string, unknown>) {
    localStorage.setItem(file, JSON.stringify(values));
}
