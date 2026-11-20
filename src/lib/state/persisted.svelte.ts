import { load } from '@tauri-apps/plugin-store';

export function persisted<T extends object>(file: string, defaults: T) {
    const state = $state(structuredClone(defaults));

    async function restore() {
        const store = await load(file, { autoSave: 100 });
        Object.assign(state, Object.fromEntries(await store.entries()));
        $effect.root(() => {
            $effect(() => {
                for (const [key, value] of Object.entries($state.snapshot(state) as object)) store.set(key, value);
            });
        });
    }

    return { state, restore };
}
