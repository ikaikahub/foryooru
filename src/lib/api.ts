import { convertFileSrc, invoke } from '@tauri-apps/api/core';

export type TagType = 'general' | 'artist' | 'copyright' | 'character' | 'metadata' | 'deprecated';
export type Tag = { name: string; type: TagType | null; count: number | null };
// alias is the name typed that the site treats as this tag, like bare_feet for barefoot
export type Suggestion = Tag & { alias?: string | null };
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
    media: Media;
    fileExt: string;
    liked: boolean;
    disliked: boolean;
    sites: SiteIds;
};

// each site's id for the post, when that site has it
export type SiteIds = { gelbooru: number | null; danbooru: number | null };

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
export type TagList = 'favouriteTags' | 'blacklist' | 'followedArtists';
export type FollowedOrder = 'latestPost' | 'recentlyFollowed' | 'name';
export type ArtistRow = { artist: Tag; posts: Post[] };
export type Site = 'gelbooru' | 'danbooru';
export type FeedOptions = { hideLiked: boolean; similarPerArtist: number | null; discover: boolean };
export type Usage = { files: number; bytes: number };
export type PostFamily = { relation: 'family' | 'similar'; posts: Post[] };
export type Signal = 'scrollBack' | 'videoLooped' | 'info' | 'shared' | 'tag' | 'similar' | 'opened';
export type StorageUsage = { database: Usage; settings: Usage; cache: Usage; cacheLimitMb: number };
export type TimingSpan = { at: number; name: string; ms: number; note: string };

export type SyncStatus = {
    running: boolean;
    lastSynced: number | null;
    pulled: number;
    error: string | null;
};

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
    | 'needsGelbooruLogin'
    | 'gelbooruFavouriteRefused'
    | 'loginWindow'
    | 'needsDanbooruLogin'
    | 'cdnBusy'
    | 'notOnCdn'
    | 'database'
    | 'io'
    | 'json'
    | 'tauri'
    | 'store'
    | 'postNotFound'
    | 'importRunning'
    | 'invalidImportFile'
    | 'noDownloadsFolder'
    | 'unsupportedSaveLocation'
    | 'unsupportedFile'
    | 'image'
    | 'clipboard'
    | 'videoServer'
    | 'gallery'
    | 'unknownTable'
    | 'archive'
    | 'notABackup';

export type AppError = { kind: ErrorKind; message: string };

export function isAppError(error: unknown): error is AppError {
    return typeof error === 'object' && error !== null && 'kind' in error && 'message' in error;
}

export function errorMessage(error: unknown) {
    return isAppError(error) ? error.message : String(error);
}

export const api = {
    waitUntilReady: () => invoke<void>('wait_until_ready'),
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
    postFamily: (id: number, chips: string[]) => invoke<PostFamily>('post_family', { id, chips }),
    fileSize: (id: number) => invoke<number | null>('file_size', { id }),
    toggleLike: (id: number, chips: string[]) => invoke<PostState>('toggle_like', { id, chips }),
    toggleDislike: (id: number) => invoke<PostState>('toggle_dislike', { id }),
    likedPosts: (chips: string[], page: number) => invoke<Listing>('liked_posts', { chips, page }),
    dislikedPosts: (chips: string[], page: number) => invoke<Listing>('disliked_posts', { chips, page }),
    followedRows: (chips: string[], order: FollowedOrder) => invoke<ArtistRow[]>('followed_rows', { chips, order }),
    followedPosts: (chips: string[], page: number) => invoke<Listing>('followed_posts', { chips, page }),
    refreshFollowed: (chips: string[]) => invoke<void>('refresh_followed', { chips }),
    relevantArtists: (artistChips: string[], chips: string[], skip: string[], limit: number) =>
        invoke<string[]>('relevant_artists', { artistChips, chips, skip, limit }),
    artistRows: (chips: string[], names: string[]) => invoke<ArtistRow[]>('artist_rows', { chips, names }),
    refreshArtists: (chips: string[], names: string[]) => invoke<void>('refresh_artists', { chips, names }),
    setSiteFavourite: (id: number, favourite: boolean) => invoke<void>('set_site_favourite', { id, favourite }),
    gelbooruLogin: () => invoke<GelbooruSession | null>('gelbooru_login'),
    gelbooruLogout: () => invoke<void>('gelbooru_logout'),
    clearLikes: () => invoke<void>('clear_likes'),
    suggestTags: (typed: string) => invoke<Tag[]>('suggest_tags', { typed }),
    suggestTagsOnline: (typed: string) => invoke<Suggestion[]>('suggest_tags_online', { typed }),
    tagsNamed: (names: string[]) => invoke<Tag[]>('tags_named', { names }),
    tagList: (list: TagList) => invoke<Tag[]>('tag_list', { list }),
    toggleTag: (list: TagList, name: string) => invoke<Tag[]>('toggle_tag', { list, name }),
    addTags: (list: TagList, names: string) => invoke<Tag[]>('add_tags', { list, names }),
    savedSearches: () => invoke<SavedSearch[]>('saved_searches'),
    toggleSavedSearch: (query: string) => invoke<SavedSearch[]>('toggle_saved_search', { query }),
    recordImpressions: (impressions: Impression[]) => invoke<void>('record_impressions', { impressions }),
    recordSignal: (post: number, signal: Signal, tag: string | null) => invoke<void>('record_signal', { post, signal, tag }),
    recordSignals: (signals: { post: number; signal: Signal; tag: string | null }[]) =>
        invoke<void>('record_signals', { signals }),
    recordSearch: (query: string) => invoke<void>('record_search', { query }),
    activity: (filter: ActivityFilter, page: number) => invoke<ActivityPage>('activity', { filter, page }),
    tableSizes: () => invoke<TableSize[]>('table_sizes'),
    tablePage: (table: string, page: number) => invoke<TablePage>('table_page', { table, page }),
    importStatus: () => invoke<ImportStatus>('import_status'),
    account: () => invoke<string | null>('account'),
    syncStatus: () => invoke<SyncStatus>('sync_status'),
    signUp: (username: string, password: string) => invoke<string>('sign_up', { username, password }),
    logIn: (username: string, password: string) => invoke<string>('log_in', { username, password }),
    logOut: () => invoke<void>('log_out'),
    importFavourites: (site: Site, user: string) => invoke<void>('import_favourites', { site, user }),
    importFromFile: (name: string, contents: string) => invoke<void>('import_from_file', { name, contents }),
    storageUsage: () => invoke<StorageUsage>('storage_usage'),
    timings: () => invoke<TimingSpan[]>('timings'),
    setTimings: (enabled: boolean) => invoke<void>('set_timings', { enabled }),
    exportData: () => invoke<SavedFile>('export_data'),
    importData: () => invoke<boolean>('import_data'),
    resetDatabase: () => invoke<void>('reset_database'),
    clearCache: () => invoke<void>('clear_cache'),
    setCacheLimit: (megabytes: number) => invoke<void>('set_cache_limit', { megabytes }),
    saveSample: (id: number) => invoke<SavedFile>('save_sample', { id }),
    saveOriginal: (id: number) => invoke<SavedFile>('save_original', { id }),
    saveOriginalAs: (id: number) => invoke<SavedFile | null>('save_original_as', { id }),
    pickSaveFolder: () => invoke<SaveFolder | null>('pick_save_folder'),
    defaultSaveFolder: () => invoke<string>('default_save_folder'),
    copyImage: (id: number) => invoke<void>('copy_image', { id }),
};

const imageBase = convertFileSrc('', 'img');

export function imageUrl(kind: 'thumbnail' | 'sample' | 'original', id: number) {
    return `${imageBase}${kind}/${id}`;
}

export const showsOriginal = (img: EventTarget, id: number) =>
    img instanceof HTMLImageElement && img.currentSrc.split('?')[0] === imageUrl('original', id);

let videoBase: string | null = null;

export async function loadVideoBase() {
    videoBase = await invoke<string | null>('video_base');
}

export const videoUrl = (id: number) =>
    videoBase ? `${videoBase}original/${id}` : imageUrl('original', id);
