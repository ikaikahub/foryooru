<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, imageUrl, type ArtistRow, type FollowedOrder } from '$lib/api';
  import { image } from '$lib/attachments';
  import ErrorNote from '$lib/components/ErrorNote.svelte';
  import ReactedPosts from '$lib/components/ReactedPosts.svelte';
  import SelectMenu from '$lib/components/SelectMenu.svelte';
  import SideScroller from '$lib/components/SideScroller.svelte';
  import { setParams } from '$lib/navigation';
  import { library } from '$lib/state/library.svelte';
  import { onRerun } from '$lib/state/rerun.svelte';
  import { ARTIST_PARAM, artistSearch, splitQuery, staged, urlQuery } from '$lib/state/search.svelte';
  import { viewer } from '$lib/state/viewer.svelte';
  import { untrack } from 'svelte';

  const ORDERS: { value: FollowedOrder; label: string }[] = [
    { value: 'latestPost', label: 'Latest post' },
    { value: 'recentlyFollowed', label: 'Recently followed' },
    { value: 'name', label: 'Name' },
  ];
  // artists found per search, and per press of See more
  const ARTISTS_PER_PAGE = 10;

  let order = $state<FollowedOrder>('latestPost');
  // in the address so Back returns to the rows
  const viewAll = $derived(page.url.searchParams.get('view') === 'all');
  let rows = $state<ArtistRow[] | null>(null);
  let error = $state<unknown>(null);
  let refreshing = $state(false);
  // what the page is waiting on, shown next to the spinner
  let fetching = $state('');

  const chips = $derived(splitQuery(urlQuery(page.url)));
  // the artist search picks which artists show; without one the page lists the followed artists
  const artistQuery = $derived(page.url.searchParams.get(ARTIST_PARAM) ?? '');
  const artistChips = $derived(splitQuery(artistQuery));
  const finding = $derived(artistChips.length > 0);
  const followedOnly = $derived(page.url.searchParams.get('followed') === 'only');
  let found = $state<string[]>([]);
  let moreToFind = $state(false);
  // a newer search makes an older one still fetching stop, so its artists don't land in the new list
  let searches = 0;

  $effect(() => {
    const current = artistQuery;
    untrack(() => artistSearch.load(current));
  });

  async function loadRows() {
    try {
      rows = finding ? await api.artistRows(chips, found) : await api.followedRows(chips, order);
      error = null;
    } catch (caught) {
      error = caught;
    }
  }

  // shows what's stored straight away, then asks the site for each artist's newest posts
  async function refresh() {
    refreshing = true;
    fetching = 'Checking followed artists for new posts';
    await loadRows();
    try {
      await api.refreshFollowed(chips);
    } catch (caught) {
      error = caught;
    }
    await loadRows();
    refreshing = false;
  }

  // the next ARTISTS_PER_PAGE artists for the search, with a page of each one's newest posts
  async function findArtists(fresh: boolean) {
    const search = ++searches;
    refreshing = true;
    try {
      if (fresh) {
        found = [];
        rows = null;
      }
      fetching = fresh ? `Searching ${artistChips.join(' ')}` : 'Finding more artists';
      const next = await api.relevantArtists(artistChips, chips, found, ARTISTS_PER_PAGE, followedOnly);
      if (search !== searches) return;
      moreToFind = next.length === ARTISTS_PER_PAGE;
      found = [...found, ...next];
      await loadRows();
      fetching = `Getting posts from ${next.length} ${next.length === 1 ? 'artist' : 'artists'}`;
      await api.refreshArtists(chips, next);
      if (search !== searches) return;
      await loadRows();
    } catch (caught) {
      error = caught;
    }
    if (search === searches) refreshing = false;
  }

  const rerun = () => (finding ? findArtists(true) : refresh());

  $effect(() => {
    void [chips, artistChips, followedOnly, finding || library.followedArtists.length];
    untrack(rerun);
  });

  $effect(() => {
    void order;
    if (!untrack(() => finding)) untrack(loadRows);
  });

  onRerun(rerun);
</script>

<div class="mb-2 flex flex-wrap items-center gap-2">
  <h1 class="mr-auto min-w-0 text-lg font-semibold break-words">
    🎨 Artists{#if finding}&nbsp;related to <span class="font-mono">{artistChips.join(' ')}</span>{/if}
  </h1>
  {#if refreshing}
    <span class="flex items-center gap-1.5 text-sm opacity-70">
      <span class="loading loading-spinner loading-sm"></span>
      {fetching}…
    </span>
  {/if}
  {#if finding}
    <button
      class={['btn btn-sm', followedOnly && 'btn-primary']}
      aria-pressed={followedOnly}
      title="Only show artists you follow"
      onclick={() => setParams({ followed: followedOnly ? null : 'only' })}
    >
      Only followed
    </button>
  {:else}
    {#if !viewAll}
      <label>
        Sort artists by:
        <SelectMenu bind:value={order} options={ORDERS} buttonClass="btn-sm" alignEnd title="Sort artists" />
      </label>
    {/if}
    <button class={['btn btn-sm', viewAll && 'btn-primary']} aria-pressed={viewAll} onclick={() => setParams({ view: viewAll ? null : 'all', page: null })}>
      {viewAll ? 'By artist' : 'View all'}
    </button>
  {/if}
</div>

{#if error}<ErrorNote {error} />{/if}

{#if viewAll && !finding}
  <ReactedPosts
    title=""
    emptyMessage={chips.length
      ? `None of their posts match your search: ${chips.join(' ')}`
      : 'No posts from the artists you follow yet.'}
    load={api.followedPosts}
  />
{:else if rows && !rows.length}
  <p class="opacity-70">
    {#if finding}
      No {followedOnly ? 'followed ' : ''}artists found for {artistChips.join(' ')} yet.
    {:else}
      Follow artists from the viewer or TikTok to see their newest posts here, or find artists with the search.
    {/if}
  </p>
{:else if rows}
  <div class="flex flex-col gap-4">
    {#each rows as row (row.artist.name)}
      {@const following = library.followed.has(row.artist.name)}
      <section>
        <div class="mb-1 flex items-center gap-2">
          <button class="link truncate font-semibold link-hover" title="Search this artist" onclick={() => goto(staged.searchHref(row.artist.name))}>
            🎨 {row.artist.name.replaceAll('_', ' ')}
          </button>
          <button class={['btn btn-xs', following ? 'btn-ghost' : 'btn-primary']} onclick={() => library.toggleTag('followedArtists', row.artist.name)}>
            {following ? 'Unfollow' : 'Follow'}
          </button>
        </div>
        {#if row.posts.length}
          <SideScroller class="flex gap-1">
            {#each row.posts as post (post.id)}
              <button
                class="h-40 shrink-0 cursor-pointer overflow-hidden rounded bg-base-300"
                style:aspect-ratio="{post.previewWidth || 1} / {post.previewHeight || 1}"
                onclick={() => viewer.open(post.id, row.posts.map((shown) => shown.id))}
              >
                <img class="size-full object-cover" alt="" {@attach image(imageUrl('sample', post.id))} />
              </button>
            {/each}
          </SideScroller>
        {:else}
          <p class="text-sm opacity-60">
            {#if refreshing}
              Looking for posts…
            {:else if chips.length}
              None of their posts match your search: {chips.join(' ')}
            {:else}
              No posts stored for this artist yet.
            {/if}
          </p>
        {/if}
      </section>
    {/each}
    {#if finding && moreToFind}
      <button class="btn btn-sm self-center" disabled={refreshing} onclick={() => findArtists(false)}>See more</button>
    {/if}
  </div>
{:else if !error}
  <span class="loading loading-spinner"></span>
{/if}
