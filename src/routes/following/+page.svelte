<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, errorMessage, imageUrl, type ArtistRow, type FollowedOrder } from '$lib/api';
  import { image } from '$lib/attachments';
  import ErrorNote from '$lib/components/ErrorNote.svelte';
  import ReactedPosts from '$lib/components/ReactedPosts.svelte';
  import SelectMenu from '$lib/components/SelectMenu.svelte';
  import { library } from '$lib/state/library.svelte';
  import { onRerun } from '$lib/state/rerun.svelte';
  import { searchHref, splitQuery, urlQuery } from '$lib/state/search.svelte';
  import { viewer } from '$lib/state/viewer.svelte';
  import { untrack } from 'svelte';

  const ORDERS: { value: FollowedOrder; label: string }[] = [
    { value: 'latestPost', label: 'Latest post' },
    { value: 'recentlyFollowed', label: 'Recently followed' },
    { value: 'name', label: 'Name' },
  ];

  let order = $state<FollowedOrder>('latestPost');
  let viewAll = $state(false);
  let rows = $state<ArtistRow[] | null>(null);
  let error = $state<unknown>(null);
  let refreshing = $state(false);

  const chips = $derived(splitQuery(urlQuery(page.url)));

  async function loadRows() {
    try {
      rows = await api.followedRows(chips, order);
      error = null;
    } catch (caught) {
      error = caught;
    }
  }

  // shows what's stored straight away, then asks the site for each artist's newest posts
  async function refresh() {
    refreshing = true;
    await loadRows();
    try {
      await api.refreshFollowed(chips);
    } catch (caught) {
      error = caught;
    }
    await loadRows();
    refreshing = false;
  }

  $effect(() => {
    void [chips, library.followedArtists.length];
    untrack(refresh);
  });

  $effect(() => {
    void order;
    untrack(loadRows);
  });

  onRerun(refresh);
</script>

<div class="mb-2 flex flex-wrap items-center gap-2">
  <h1 class="mr-auto text-lg font-semibold">👥 Following</h1>
  {#if refreshing}<span class="loading loading-spinner loading-sm" title="Checking for new posts"></span>{/if}
  {#if !viewAll}
    <SelectMenu bind:value={order} options={ORDERS} buttonClass="btn-sm" alignEnd title="Sort artists" />
  {/if}
  <button class={['btn btn-sm', viewAll && 'btn-primary']} aria-pressed={viewAll} onclick={() => (viewAll = !viewAll)}>
    {viewAll ? 'By artist' : 'View all'}
  </button>
</div>

{#if error}<ErrorNote {error} />{/if}

{#if viewAll}
  <ReactedPosts title="" emptyMessage="No posts from the artists you follow yet." load={api.followedPosts} />
{:else if rows && !rows.length}
  <p class="opacity-70">Follow artists from the viewer or TikTok to see their newest posts here.</p>
{:else if rows}
  <div class="flex flex-col gap-4">
    {#each rows as row (row.artist.name)}
      <section>
        <div class="mb-1 flex items-center gap-2">
          <button class="link truncate font-semibold link-hover" title="Search this artist" onclick={() => goto(searchHref('/', row.artist.name))}>
            🎨 {row.artist.name.replaceAll('_', ' ')}
          </button>
          <button class="btn btn-ghost btn-xs" onclick={() => library.toggleTag('followedArtists', row.artist.name)}>Unfollow</button>
        </div>
        {#if row.posts.length}
          <div class="flex gap-1 overflow-x-auto pb-1">
            {#each row.posts as post (post.id)}
              <button
                class="h-40 shrink-0 cursor-pointer overflow-hidden rounded bg-base-300"
                style:aspect-ratio="{post.previewWidth || 1} / {post.previewHeight || 1}"
                onclick={() => viewer.open(post.id, row.posts.map((shown) => shown.id))}
              >
                <img class="size-full object-cover" alt="" {@attach image(imageUrl('sample', post.id))} />
              </button>
            {/each}
          </div>
        {:else}
          <p class="text-sm opacity-60">{refreshing ? 'Looking for posts…' : 'No posts stored for this artist yet.'}</p>
        {/if}
      </section>
    {/each}
  </div>
{:else if !error}
  <span class="loading loading-spinner"></span>
{/if}
