<script lang="ts">
  import { page } from '$app/state';
  import type { Listing } from '$lib/api';
  import { Loader } from '$lib/loader.svelte';
  import { hrefWith, pageNumber } from '$lib/navigation';
  import { onRerun } from '$lib/state/rerun.svelte';
  import { splitQuery, urlQuery } from '$lib/state/search.svelte';
  import ListingView from './ListingView.svelte';

  let {
    title,
    emptyMessage,
    load,
  }: { title: string; emptyMessage: string; load: (chips: string[], pageIndex: number) => Promise<Listing> } = $props();

  const current = $derived(pageNumber(page.url));
  const chips = $derived(splitQuery(urlQuery(page.url)));
  const loadPage = $derived((pageIndex: number) => load(chips, pageIndex));
  const listing = new Loader<Listing>();

  $effect(() => {
    const [pageIndex, loadIt] = [current - 1, loadPage];
    listing.run(() => loadIt(pageIndex));
  });

  onRerun(() => listing.run(() => loadPage(current - 1)));
</script>

<h1 class="mb-2 text-lg font-semibold">{title}</h1>
<ListingView loader={listing} load={loadPage} {current} href={(number) => hrefWith(page.url, { page: number })} loadingText="Loading…" {emptyMessage} />
