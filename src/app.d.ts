declare global {
    const __BUILD__: string;

    namespace App {
        interface PageState {
            post?: number;
            ids?: number[];
            depth?: number;
            turned?: boolean;
            listPage?: number;
            searching?: boolean;
            menuOpen?: boolean;
            feedInfo?: boolean;
            viewerInfo?: boolean;
            viewerTags?: boolean;
        }
    }
}

export {};
