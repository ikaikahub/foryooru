<script lang="ts">
  import EmptyState from './EmptyState.svelte';
  import type { Listing } from '$lib/api';
  import { hiddenNote } from '$lib/format';
  import type { Loader } from '$lib/loader.svelte';
  import { library } from '$lib/state/library.svelte';
  import type { PageTurner } from '$lib/state/viewer.svelte';
  import ErrorNote from './ErrorNote.svelte';
  import LoadingBar from './LoadingBar.svelte';
  import Pager from './Pager.svelte';
  import PostGrid from './PostGrid.svelte';

  let {
    loader,
    load,
    current,
    href,
    loadingText,
    emptyMessage = 'No posts found.',
    trackSeen = true,
  }: {
    loader: Loader<Listing>;
    load: (pageIndex: number) => Promise<Listing>;
    current: number;
    href: (page: number) => string;
    loadingText: string;
    emptyMessage?: string;
    trackSeen?: boolean;
  } = $props();

  const listing = $derived(loader.data);
  const note = $derived(listing && hiddenNote(listing.hidden));
  const pageTurner = $derived<PageTurner | null>(
    listing && {
      current,
      pages: listing.pages,
      href,
      load: async (number) => {
        const found = await load(number - 1);
        library.remember(found.posts);
        return found.posts.map((post) => post.id);
      },
    },
  );

  $effect(() => {
    if (listing) library.remember(listing.posts);
  });
</script>

{#if loader.error}
  <ErrorNote error={loader.error} centered />
{:else}
  {#if loader.loading}<LoadingBar text={loadingText} />{/if}
  {#if listing}
    {#if note}<p class="mb-1 text-xs opacity-70">{note}</p>{/if}
    {#if listing.posts.length}
      <PostGrid posts={listing.posts} {pageTurner} {trackSeen} />
    {:else}
      <EmptyState message={emptyMessage} />
    {/if}
    <Pager {current} pages={listing.pages} {href} />
  {/if}
{/if}
