// github.com release downloads have no CORS headers, but the API does
export const RELEASES_API = 'https://api.github.com/repos/ikaikahub/foryooru/releases';

// body holds the release notes, a list of the commit messages since the release before
export type Release = {
    tag_name: string;
    published_at: string;
    body: string | null;
    assets: { name: string; browser_download_url: string }[];
};
