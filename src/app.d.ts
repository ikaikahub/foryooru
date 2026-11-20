import type { Site } from '$lib/api';

declare global {
    const __BUILD__: string;

    namespace App {
        interface PageState {
            post?: number;
            ids?: number[];
            depth?: number;
            turned?: boolean;
            fromUrl?: boolean;
            site?: Site;
            listPage?: number;
            searching?: boolean;
            menuOpen?: boolean;
        }
    }
}

export {};
