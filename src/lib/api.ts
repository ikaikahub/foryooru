import { SvelteMap } from 'svelte/reactivity';
import { exportBackup, importBackup, invoke as callEngine, ready } from '$lib/engine/client';
import { overwritePersisted, snapshotPersisted } from '$lib/state/persisted.svelte';
import { settings } from '$lib/state/settings.svelte';
import { GELBOORU_IMAGES } from '$lib/sites';

export type TagType = 'general' | 'artist' | 'copyright' | 'character' | 'metadata' | 'deprecated';
export type Tag = { name: string; type: TagType | null; count: number | null };
export type TagGroup = { label: string; tags: Tag[] };
export type Media = 'image' | 'gif' | 'video';

export type Post = {
    id: number;
    rating: string;
    score: number;
    width: number;
    height: number;
    previewWidth: number;
    previewHeight: number;
    postedAt: number | null;
    fileUrl: string;
    previewUrl: string;
    sampleUrl: string;
    media: Media;
    fileExt: string;
    liked: boolean;
    disliked: boolean;
};

export type PostDetail = Post & { tagGroups: TagGroup[]; blockedBy: string[] };
export type PostState = { id: number; liked: boolean; disliked: boolean };
export type SaveFolder = { location: string; label: string };
export type SavedFile = { name: string; folder: string };
export type GelbooruSession = { userId: string; passHash: string };
export type HiddenTag = { name: string; count: number };
export type Listing = { posts: Post[]; pages: number; hidden: HiddenTag[] };
export type Ranked = { id: number; explore: boolean };
export type MoreRanked = { ranked: Ranked[]; broadened: string[] | null };
export type FeedBatch = { posts: PostDetail[]; exhausted: boolean; broadened: string[] | null };
export type SavedSearch = { query: string; tags: Tag[] };
export type TagList = 'favouriteTags' | 'blacklist';
export type Site = 'gelbooru' | 'danbooru';
export type FeedOptions = { hideLiked: boolean; similarPerArtist: number | null; discover: boolean };
export type Usage = { files: number; bytes: number };
export type PostFamily = { relation: 'family' | 'similar'; posts: Post[] };
export type Signal = 'scrollBack' | 'videoLooped' | 'info' | 'shared' | 'tag' | 'similar' | 'opened';
export type ModelStatus = { learned: number; graded: number; needed: number; model: number | null; current: number | null; trust: number };
export type LeadTag = { tag: string; views: number; factor: number };
export type FetchStats = {
    fetched: number;
    judged: number;
    fetchedRate: number | null;
    overallRate: number | null;
    best: LeadTag[];
    worst: LeadTag[];
};
export type AlgorithmOverview = { posts: number; candidates: number; unseen: number; model: ModelStatus; fetching: FetchStats };
export type StorageUsage = { database: Usage; settings: Usage; cache: Usage; cacheLimitMb: number };

export type ImportStatus = {
    running: boolean;
    source: string;
    total: number | null;
    imported: number;
    skipped: number;
    error: string | null;
};

export type ImpressionSource = 'feed' | 'grid' | 'viewer';
export type Impression = { post: number; source: ImpressionSource; ms?: number; opened?: boolean };

export type ActivityFilter = 'all' | 'views' | 'likes' | 'dislikes' | 'queries';
export type ReactionKind = 'like' | 'unlike' | 'dislike' | 'undislike';

export type ActivityEvent =
    | { kind: 'view'; post: number; source: ImpressionSource; dwellMs: number | null; quickSkip: boolean; longLook: boolean }
    | { kind: ReactionKind; post: number }
    | { kind: 'query'; endpoint: string; query: string; page: number | null; status: number | null; error: string | null; ms: number };

export type ActivityEntry = { id: number; at: number } & ActivityEvent;
export type ActivityPage = { entries: ActivityEntry[]; total: number; pages: number };

export type TableSize = { name: string; rows: number; bytes: number };
export type TablePage = { columns: string[]; rows: (string | number | null)[][]; total: number; pages: number };

export type ErrorKind =
    | 'missingApiKey'
    | 'http'
    | 'status'
    | 'rejected'
    | 'needsGelbooru'
    | 'cdnBusy'
    | 'database'
    | 'io'
    | 'json'
    | 'tauri'
    | 'store'
    | 'postNotFound'
    | 'importRunning'
    | 'invalidImportFile'
    | 'noDownloadsFolder'
    | 'gallery'
    | 'unknownTable';

export type AppError = { kind: ErrorKind; message: string };

export function isAppError(error: unknown): error is AppError {
    return typeof error === 'object' && error !== null && 'kind' in error && 'message' in error;
}

export function errorMessage(error: unknown) {
    return isAppError(error) ? error.message : String(error);
}

export const api = {
    waitUntilReady: () => ready,
    browse: (chips: string[], page: number) => invoke<Listing>('browse', { chips, page }),
    forYou: (chips: string[], hideLiked: boolean, discover: boolean) =>
        invoke<Ranked[]>('for_you', { chips, hideLiked, discover }),
    similar: (chips: string[], perArtist: number | null) => invoke<number[]>('similar', { chips, perArtist }),
    forYouMore: (chips: string[], hideLiked: boolean, discover: boolean, shown: number[]) =>
        invoke<MoreRanked>('for_you_more', { chips, hideLiked, discover, shown }),
    similarMore: (chips: string[], perArtist: number | null, shown: number[]) =>
        invoke<number[]>('similar_more', { chips, perArtist, shown }),
    feedNext: (chips: string[], seen: number[], since: number, options: FeedOptions) =>
        invoke<FeedBatch>('feed_next', { chips, seen, since, options }),
    posts: (ids: number[]) => invoke<Post[]>('posts', { ids }),
    postDetail: (id: number) => invoke<PostDetail>('post_detail', { id }),
    openPost: (id: number) => invoke<PostDetail>('open_post', { id }),
    previewPost: (site: Site, id: number) => previewPost(site, id),
    postFamily: (id: number, chips: string[]) => invoke<PostFamily>('post_family', { id, chips }),
    fileSize: (id: number, site: Site = settings.site) => fileSize(id, site),
    toggleLike: (id: number, chips: string[]) => invoke<PostState>('toggle_like', { id, chips }),
    toggleDislike: (id: number) => invoke<PostState>('toggle_dislike', { id }),
    likedPosts: (chips: string[], page: number) => invoke<Listing>('liked_posts', { chips, page }),
    dislikedPosts: (chips: string[], page: number) => invoke<Listing>('disliked_posts', { chips, page }),
    setSiteFavourite: (id: number, favourite: boolean) => invoke<void>('set_site_favourite', { id, favourite }),
    gelbooruLogin: () => Promise.reject<GelbooruSession | null>(desktopOnly('Gelbooru login')),
    gelbooruLogout: () => Promise.resolve(),
    clearLikes: () => invoke<void>('clear_likes'),
    suggestTags: (typed: string) => invoke<Tag[]>('suggest_tags', { typed }),
    tagsNamed: (names: string[]) => invoke<Tag[]>('tags_named', { names }),
    tagList: (list: TagList) => invoke<Tag[]>('tag_list', { list }),
    toggleTag: (list: TagList, name: string) => invoke<Tag[]>('toggle_tag', { list, name }),
    addTags: (list: TagList, names: string) => invoke<Tag[]>('add_tags', { list, names }),
    savedSearches: () => invoke<SavedSearch[]>('saved_searches'),
    toggleSavedSearch: (query: string) => invoke<SavedSearch[]>('toggle_saved_search', { query }),
    recordImpressions: (impressions: Impression[], chips: string[]) => invoke<void>('record_impressions', { impressions, chips }),
    recordSignal: (post: number, signal: Signal, tag: string | null) => invoke<void>('record_signal', { post, signal, tag }),
    recordSearch: (query: string) => invoke<void>('record_search', { query }),
    activity: (filter: ActivityFilter, page: number) => invoke<ActivityPage>('activity', { filter, page }),
    tableSizes: () => invoke<TableSize[]>('table_sizes'),
    tablePage: (table: string, page: number) => invoke<TablePage>('table_page', { table, page }),
    importStatus: () => invoke<ImportStatus>('import_status'),
    algorithmOverview: (chips: string[], hideLiked: boolean) =>
        invoke<AlgorithmOverview>('algorithm_overview', { chips, hideLiked }),
    importFavourites: (user: string) => invoke<void>('import_favourites', { user }),
    setSite: (site: Site) => invoke<void>('set_site', { site }),
    importFromFile: (name: string, contents: string) => invoke<void>('import_from_file', { name, contents }),
    storageUsage: () => invoke<StorageUsage>('storage_usage'),
    exportData: () => exportData(),
    importData: () => importData(),
    resetDatabase: () => invoke<void>('reset_database'),
    resetAllDatabases: () => invoke<void>('reset_all_databases'),
    clearCache: () => Promise.resolve(),
    setCacheLimit: (_megabytes: number) => Promise.resolve(),
    saveSample: (id: number, site: Site = settings.site) => saveImage('sample', id, site),
    saveOriginal: (id: number, site: Site = settings.site) => saveImage('original', id, site),
    pickSaveFolder: () => Promise.resolve<SaveFolder | null>(null),
    defaultSaveFolder: () => Promise.resolve('Downloads'),
    copyImage: (id: number, site: Site = settings.site) => copyImage(id, site),
};

type MediaUrls = Record<'thumbnail' | 'sample' | 'original', string>;

// the same post id can exist on both sites, so cached media urls carry the site
const known = new SvelteMap<string, MediaUrls>();
const lookups = new Set<number>();
let lookupTimer: ReturnType<typeof setTimeout> | undefined;

const mediaKey = (site: Site, id: number) => `${site}:${id}`;

// Gelbooru's image servers refuse other sites' pages, so its images load through the proxy too.
// The database keeps the original addresses, so changing the proxy doesn't break stored posts.
function throughProxy(url: string) {
    if (!/^https:\/\/[a-z-]+\d*\.gelbooru\.com\//.test(url)) return url;
    return `${GELBOORU_IMAGES}/img/${url.slice('https://'.length)}`;
}

function remember(value: unknown, site: Site = settings.site) {
    if (Array.isArray(value)) {
        value.forEach((post) => remember(post, site));
    } else if (value && typeof value === 'object') {
        const post = value as Partial<Post>;
        if (typeof post.id === 'number' && typeof post.fileUrl === 'string') {
            // Gelbooru gives videos and gifs no sample, so their still is the thumbnail rather than the media file
            const moving = post.media === 'video' || post.media === 'gif';
            const sample = moving ? post.previewUrl || post.fileUrl : post.sampleUrl || post.fileUrl;
            known.set(mediaKey(site, post.id), {
                thumbnail: throughProxy(post.previewUrl || post.sampleUrl || post.fileUrl),
                sample: throughProxy(sample),
                original: throughProxy(post.fileUrl),
            });
        }
        Object.values(value).forEach((post) => remember(post, site));
    }
}

function lookUp(id: number) {
    if (lookups.has(id)) return;
    lookups.add(id);
    clearTimeout(lookupTimer);
    lookupTimer = setTimeout(() => {
        const ids = [...lookups].filter((wanted) => !known.has(mediaKey(settings.site, wanted)));
        if (ids.length) invoke<Post[]>('posts', { ids }).catch(() => {});
    });
}

async function invoke<T>(command: string, args?: Record<string, unknown>) {
    const result = await callEngine<T>(command, args);
    remember(result);
    return result;
}

// shared links show the other site's post without storing it, so the urls are kept under that site
async function previewPost(site: Site, id: number) {
    const post = await callEngine<PostDetail>('preview_post', { site, id });
    remember(post, site);
    return post;
}

export function imageUrl(kind: 'thumbnail' | 'sample' | 'original', id: number, site: Site = settings.site) {
    const urls = known.get(mediaKey(site, id));
    if (!urls && site === settings.site) lookUp(id);
    return urls?.[kind] ?? '';
}

export const showsOriginal = (img: EventTarget, id: number, site: Site = settings.site) =>
    img instanceof HTMLImageElement && !!known.get(mediaKey(site, id)) && img.currentSrc === known.get(mediaKey(site, id))?.original;

export async function loadVideoBase() {}

export const videoUrl = (id: number, site: Site = settings.site) => imageUrl('original', id, site);

function desktopOnly(feature: string): AppError {
    return { kind: 'tauri', message: `${feature} needs the desktop app` };
}

function downloadBlob(blob: Blob, name: string): SavedFile {
    const link = document.createElement('a');
    link.href = URL.createObjectURL(blob);
    link.download = name;
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(link.href), 60_000);
    return { name, folder: 'Downloads' };
}

async function mediaBlob(kind: 'sample' | 'original', id: number, site: Site) {
    const url = imageUrl(kind, id, site);
    if (!url) throw { kind: 'postNotFound', message: "That post isn't cached" } satisfies AppError;
    const response = await fetch(url);
    if (!response.ok) throw { kind: 'status', message: `The image server answered ${response.status}` } satisfies AppError;
    return { blob: await response.blob(), url };
}

async function saveImage(kind: 'sample' | 'original', id: number, site: Site) {
    const { blob, url } = await mediaBlob(kind, id, site);
    const extension = url.split('?')[0].split('.').pop() ?? 'jpg';
    const suffix = kind === 'sample' ? '_sample' : '';
    return downloadBlob(blob, `${id}${suffix}.${extension}`);
}

// browsers only accept PNG on the clipboard, so the image is redrawn as one
async function copyImage(id: number, site: Site) {
    const { blob } = await mediaBlob('original', id, site);
    const bitmap = await createImageBitmap(blob);
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0);
    const png = await canvas.convertToBlob({ type: 'image/png' });
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })]);
}

async function fileSize(id: number, site: Site) {
    const url = imageUrl('original', id, site);
    if (!url) return null;
    const length = (await fetch(url, { method: 'HEAD' })).headers.get('content-length');
    return length ? Number(length) : null;
}

async function exportData() {
    const bytes = await exportBackup(snapshotPersisted('settings.json'));
    const day = new Date().toISOString().slice(0, 10);
    return downloadBlob(new Blob([bytes as BlobPart], { type: 'application/zip' }), `foryooru-backup-${day}.zip`);
}

async function importData() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.zip,application/zip';
    const file = await new Promise<File | undefined>((resolve) => {
        input.onchange = () => resolve(input.files?.[0]);
        input.oncancel = () => resolve(undefined);
        input.click();
    });
    if (!file) return false;
    const settings = await importBackup(new Uint8Array(await file.arrayBuffer()));
    if (settings) overwritePersisted('settings.json', settings);
    return true;
}
