<script module lang="ts">
  type Kept = { ids: number[]; posts: Post[]; loaded: number; extra: number[]; ended: boolean };

  const kept = new Map<string, Kept>();

  export const BATCH = 100;
</script>

<script lang="ts">
  import EmptyState from './EmptyState.svelte';
  import { api, type Post } from '$lib/api';
  import { onVisible } from '$lib/attachments';
  import type { Loader } from '$lib/loader.svelte';
  import { library } from '$lib/state/library.svelte';
  import ErrorNote from './ErrorNote.svelte';
  import LoadingBar from './LoadingBar.svelte';
  import PostGrid from './PostGrid.svelte';
  import { untrack } from 'svelte';

  let {
    loader,
    keep,
    loadingText,
    emptyMessage,
    endMessage,
    more,
  }: {
    loader: Loader<number[]>;
    keep?: string;
    loadingText: string;
    emptyMessage: string;
    endMessage: string;
    more?: (shown: number[]) => Promise<number[]>;
  } = $props();

  let posts = $state<Post[]>([]);
  let loaded = $state(0);
  let extra = $state<number[]>([]);
  let ended = $state(false);
  let busy = false;
  let generation = 0;

  const ids = $derived(loader.data);
  const all = $derived(ids ? [...ids, ...extra] : null);
  const canGrow = $derived(all !== null && (loaded < all.length || (more !== undefined && !ended)));

  $effect(() => {
    const current = ids;
    untrack(() => {
      generation++;
      busy = false;
      const saved = keep ? kept.get(keep) : undefined;
      if (current && saved?.ids === current) {
        ({ posts, loaded, extra, ended } = saved);
        return;
      }
      posts = [];
      loaded = 0;
      extra = [];
      ended = false;
      if (current) loadMore();
    });
  });

  $effect(() => {
    if (!keep || !ids) return;
    kept.set(keep, { ids, posts, loaded, extra, ended });
  });

  async function loadMore() {
    const from = all;
    if (!from || busy) return;
    busy = true;
    const run = generation;
    try {
      if (loaded < from.length) {
        const batch = from.slice(loaded, loaded + BATCH);
        const found = await api.posts(batch);
        if (run !== generation) return;
        library.remember(found);
        posts = [...posts, ...found];
        loaded += batch.length;
      } else if (more && !ended) {
        const shown = new Set(from);
        const fresh = (await more(from)).filter((id) => !shown.has(id));
        if (run !== generation) return;
        if (fresh.length) extra = [...extra, ...fresh];
        else ended = true;
      }
    } finally {
      if (run === generation) busy = false;
    }
  }
</script>

{#if loader.error}
  <ErrorNote error={loader.error} centered />
{:else}
  {#if loader.loading}<LoadingBar text={loadingText} />{/if}
  {#if all}
    {#if all.length}<PostGrid {posts} />{/if}
    {#if canGrow}
      {#key `${loaded}/${all.length}`}
        <div class="flex items-center justify-center gap-2 py-6 text-sm opacity-60" {@attach onVisible(loadMore)}>
          <span class="loading loading-spinner loading-sm"></span>Loading more…
        </div>
      {/key}
    {:else if all.length}
      <p class="py-6 text-center text-sm opacity-60">{endMessage}</p>
    {:else}
      <EmptyState message={emptyMessage} />
    {/if}
  {/if}
{/if}
