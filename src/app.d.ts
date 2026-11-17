declare global {
    namespace App {
        interface PageState {
            post?: number;
            ids?: number[];
            depth?: number;
            turned?: boolean;
            listPage?: number;
            searching?: boolean;
            menuOpen?: boolean;
        }
    }
}

export {};
