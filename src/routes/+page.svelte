<script module lang="ts">
  const similarRankings = new Map<string, number[]>();
</script>

<script lang="ts">
  import { page } from '$app/state';
  import { api, type Listing } from '$lib/api';
  import ListingView from '$lib/components/ListingView.svelte';
  import { Loader } from '$lib/loader.svelte';
  import { hrefWith, pageNumber } from '$lib/navigation';
  import { onRerun } from '$lib/state/rerun.svelte';
  import { similarId, splitQuery, urlQuery } from '$lib/state/search.svelte';
  import { settings } from '$lib/state/settings.svelte';

  const PAGE_SIZE = 100;

  const chips = $derived(splitQuery(urlQuery(page.url)));
  const current = $derived(pageNumber(page.url));
  const similarMode = $derived(chips.some((chip) => !chip.startsWith('-') && similarId(chip) !== null));

  const listing = new Loader<Listing>();

  const similarKey = (search: string[]) => `${search.join(' ')}\n${settings.similarPerArtist}`;

  async function similarRanking(search: string[]) {
    const remembered = similarRankings.get(similarKey(search));
    if (remembered) return remembered;
    const ranked = await api.similar(search, settings.similarPerArtist);
    similarRankings.set(similarKey(search), ranked);
    return ranked;
  }

  async function similarPage(search: string[], pageIndex: number): Promise<Listing> {
    let ranked = await similarRanking(search);
    if ((pageIndex + 1) * PAGE_SIZE >= ranked.length) {
      const shown = new Set(ranked);
      const found = await api.similarMore(search, settings.similarPerArtist, ranked);
      ranked = [...ranked, ...found.filter((id) => !shown.has(id))];
      similarRankings.set(similarKey(search), ranked);
    }
    const ids = ranked.slice(pageIndex * PAGE_SIZE, (pageIndex + 1) * PAGE_SIZE);
    return { posts: await api.posts(ids), pages: Math.max(1, Math.ceil(ranked.length / PAGE_SIZE)), hidden: [] };
  }

  const load = (pageIndex: number) => (similarMode ? similarPage(chips, pageIndex) : api.browse(chips, pageIndex));

  $effect(() => {
    const pageIndex = current - 1;
    listing.run(() => load(pageIndex));
  });

  onRerun(() => {
    similarRankings.delete(similarKey(chips));
    listing.run(() => load(current - 1));
  });
</script>

<ListingView
  loader={listing}
  {load}
  {current}
  href={(number) => hrefWith(page.url, { page: number })}
  loadingText={similarMode ? 'Finding similar posts…' : 'Searching…'}
  emptyMessage={similarMode ? 'No similar posts found for this search.' : undefined}
  trackSeen={false}
/>
