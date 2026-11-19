export type AlgorithmPage = 'forYou' | 'feed';

export const DISCOVER_KEYS = {
    forYou: 'forYouDiscover',
    feed: 'feedDiscover',
} as const;

export function algorithmPageFor(path: string): AlgorithmPage | null {
    if (path === '/for-you') return 'forYou';
    if (path === '/tiktok') return 'feed';
    return null;
}
