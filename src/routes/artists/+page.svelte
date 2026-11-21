<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, imageUrl, type ArtistRow, type FollowedOrder } from '$lib/api';
  import { image, onVisible } from '$lib/attachments';
  import ErrorNote from '$lib/components/ErrorNote.svelte';
  import ReactedPosts from '$lib/components/ReactedPosts.svelte';
  import SelectMenu from '$lib/components/SelectMenu.svelte';
  import SideScroller from '$lib/components/SideScroller.svelte';
  import { setParams } from '$lib/navigation';
  import { library } from '$lib/state/library.svelte';
  import { onRerun } from '$lib/state/rerun.svelte';
  import { FILTER_PARAM, RATING_BUTTONS, bare, postFilter, splitQuery, staged, urlQuery } from '$lib/state/search.svelte';
  import { viewer } from '$lib/state/viewer.svelte';
  import TagChip from '$lib/components/TagChip.svelte';
  import TagInput from '$lib/components/TagInput.svelte';
  import { untrack } from 'svelte';

  const ORDERS: { value: FollowedOrder; label: string }[] = [
    { value: 'latestPost', label: 'Latest post' },
    { value: 'recentlyFollowed', label: 'Recently followed' },
    { value: 'name', label: 'Name' },
  ];
  // artists found per search, and per press of See more
  const ARTISTS_PER_PAGE = 10;
  // grey squares in a row still waiting for its posts; enough to fill a wide screen
  const PLACEHOLDER_TILES = 8;
  // the next artists start loading this far before the bottom comes into view, like For you
  const LOAD_AHEAD = '0px 0px 1200px 0px';

  let order = $state<FollowedOrder>('latestPost');
  // in the address so Back returns to the rows
  const viewAll = $derived(page.url.searchParams.get('view') === 'all');
  let rows = $state<ArtistRow[] | null>(null);
  let error = $state<unknown>(null);
  let refreshing = $state(false);
  // what the page is waiting on, shown next to the spinner
  let fetching = $state('');

  // the page's search picks which artists show, and without one the page lists the followed artists. both go through
  // their text, so other address changes (the viewer adds ?post=) don't count as a new search
  const query = $derived(urlQuery(page.url));
  const artistChips = $derived(splitQuery(query));
  const finding = $derived(artistChips.length > 0);
  // the title names what the artists were found for; ratings are only a limit on that
  const titleTags = $derived(artistChips.filter((chip) => !bare(chip).startsWith('rating:')));
  // the Filter & sort menu's tags, -tags and ratings filter the posts shown for those artists
  const filterQuery = $derived(page.url.searchParams.get(FILTER_PARAM) ?? '');
  const chips = $derived(splitQuery(filterQuery));
  let found = $state<string[]>([]);
  let moreToFind = $state(false);
  // a newer search makes an older one still fetching stop, so its artists don't land in the new list
  let searches = 0;

  $effect(() => {
    const current = filterQuery;
    untrack(() => postFilter.load(current));
  });

  // filter changes apply as they're made; replacing the history entry keeps Back for leaving the page
  $effect(() => {
    const changed = postFilter.query;
    if (changed !== untrack(() => filterQuery)) setParams({ [FILTER_PARAM]: changed || null }, { replace: true });
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
      const next = await api.relevantArtists(artistChips, chips, found, ARTISTS_PER_PAGE);
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

  // following or unfollowing from a row leaves the list as it is until the next visit or search
  $effect(() => {
    void [chips, artistChips];
    untrack(rerun);
  });

  // artists with nothing to show are left out once loading is done; until then they hold their place
  const shownRows = $derived(rows && !refreshing ? rows.filter((row) => row.posts.length) : rows);

  $effect(() => {
    void order;
    if (!untrack(() => finding)) untrack(loadRows);
  });

  onRerun(rerun);
</script>

<div class="mb-2 flex flex-wrap items-center gap-2">
  <!-- <h1 class="mr-auto min-w-0 text-lg font-semibold break-words">
    🎨 
    {#if !finding}Followed artists{/if}
    {#if finding}Artists related to <span class="font-mono">{titleTags.join(' ')}</span>{/if}
  </h1> -->
  {#if refreshing}
    <span class="flex items-center gap-1.5 text-sm opacity-70">
      <span class="loading loading-spinner loading-sm"></span>
      {fetching}…
    </span>
  {/if}
  {#if !finding}
    <button class={['btn btn-sm', viewAll && 'btn-primary']} aria-pressed={viewAll} onclick={() => setParams({ view: viewAll ? null : 'all', page: null })}>
      {viewAll ? 'By artist' : 'View all'}
    </button>
  {/if}
  <button
    class={['btn btn-square btn-sm', chips.length && 'btn-primary']}
    popovertarget="artist-filter"
    style:anchor-name="--artist-filter"
    aria-label="Filter and sort"
    title={chips.length ? `Filtered: ${chips.join(' ')}` : 'Filter and sort'}
  >
    <!-- a funnel -->
    <svg class="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M3 5h18l-7 8v6l-4 2v-8z" />
    </svg>
  </button>
  <!-- filters the posts shown for each artist; the page's search picks the artists. overflow stays visible so the
       tag suggestions can drop out of it -->
  <div
    id="artist-filter"
    popover
    class="dropdown dropdown-end w-80 max-w-[calc(100vw-1rem)] flex-col gap-2 overflow-visible rounded-box bg-base-200 p-3 shadow-lg [&:popover-open]:flex"
    style:position-anchor="--artist-filter"
  >
    <div class="flex gap-1">
      {#each RATING_BUTTONS as rating (rating)}
        {@const chip = `rating:${rating}`}
        <button
          class={['btn btn-sm min-w-0 flex-1 px-1 text-xs capitalize', postFilter.state(chip) === 'include' && 'btn-primary']}
          title={rating}
          onclick={() => postFilter.toggle(chip)}><span class="truncate">{rating}</span></button
        >
      {/each}
    </div>
    <div class="flex gap-1">
      <button class="btn btn-square btn-sm" title="Clear the filter" onmousedown={(e) => e.preventDefault()} onclick={() => postFilter.clear()}>🧹</button>
      <div class="min-w-0 flex-1"><TagInput editor={postFilter} placeholder="Filter their posts…" onsubmit={() => postFilter.commitTyped()} /></div>
    </div>
    {#if postFilter.chips.length}
      <div class="flex flex-wrap gap-1">
        {#each postFilter.chips as chip (chip.tag)}
          <TagChip tag={chip.tag} type={chip.type} title="Include / exclude" onclick={() => postFilter.toggleExclude(chip)}>
            <button
              type="button"
              class="-my-1 flex min-w-7 cursor-pointer items-center justify-center self-stretch py-1 pr-1 text-base opacity-60 hover:opacity-100"
              aria-label="Remove"
              onclick={() => postFilter.remove(bare(chip.tag))}>×</button
            >
          </TagChip>
        {/each}
      </div>
    {/if}
    {#if !finding && !viewAll}
      <label class="flex items-center gap-2 text-sm">
        Sort followed artists by:
        <SelectMenu bind:value={order} options={ORDERS} buttonClass="btn-sm" title="Sort artists" />
      </label>
    {/if}
  </div>
</div>

{#if error}<ErrorNote {error} />{/if}

{#if viewAll && !finding}
  <ReactedPosts
    title=""
    emptyMessage={chips.length
      ? `None of their posts match your filter: ${chips.join(' ')}`
      : 'No posts from the artists you follow yet.'}
    load={(_, pageIndex) => api.followedPosts(chips, pageIndex)}
  />
{:else if rows && shownRows && !shownRows.length}
  <p class="opacity-70">
    {#if rows.length && chips.length}
      None of their posts match your filter: {chips.join(' ')}
    {:else if finding}
      No artists found for {artistChips.join(' ')} yet.
    {:else if rows.length}
      No posts stored for the artists you follow yet.
    {:else}
      Follow artists from the viewer or TikTok to see their newest posts here, or find artists with the search.
    {/if}
  </p>
{:else if shownRows}
  <div class="flex flex-col gap-4">
    {#each shownRows as row (row.artist.name)}
      {@const following = library.followed.has(row.artist.name)}
      <section>
        <div class="mb-1 flex items-center gap-2">
          <button class="link truncate font-semibold link-hover" title="Search this artist" onclick={() => goto(staged.searchHref(row.artist.name))}>
            🎨 {row.artist.name.replaceAll('_', ' ')}
          </button>
          <button class={['btn btn-xs', !following && 'btn-primary']} onclick={() => library.toggleTag('followedArtists', row.artist.name)}>
            {following ? 'Unfollow' : 'Follow'}
          </button>
          {#if query !== row.artist.name}
            <button
              class="btn btn-xs"
              title="Artists this artist's fans also like"
              onclick={() => setParams({ q: row.artist.name, view: null })}
            >
              Similar artists
            </button>
          {/if}
        </div>
        {#if row.posts.length}
          <SideScroller class="flex gap-1">
            {#each row.posts as post (post.id)}
              <button
                class="h-40 shrink-0 cursor-pointer overflow-hidden rounded bg-base-300"
                style:aspect-ratio="{post.previewWidth || 1} / {post.previewHeight || 1}"
                onclick={() => viewer.open(post.id, row.posts.map((shown) => shown.id))}
              >
                <img class="size-full object-cover" alt="" {@attach image(imageUrl('thumbnail', post.id))} />
              </button>
            {/each}
          </SideScroller>
        {:else}
          <!-- as tall as a row of posts, so the page doesn't jump when they arrive -->
          <div class="flex h-40 gap-1 overflow-hidden" aria-label="Looking for posts">
            {#each { length: PLACEHOLDER_TILES } as _, index (index)}
              <div class="aspect-square h-40 shrink-0 animate-pulse rounded bg-base-300"></div>
            {/each}
          </div>
        {/if}
      </section>
    {/each}
    {#if finding && moreToFind}
      <!-- a new button after each load and when loading stops, so being in view can load more again -->
      {#key `${found.length}/${refreshing}`}
        <button
          class="btn btn-sm self-center"
          disabled={refreshing}
          onclick={() => findArtists(false)}
          {@attach onVisible(() => !refreshing && findArtists(false), LOAD_AHEAD)}>See more</button
        >
      {/key}
    {/if}
  </div>
{:else if !error}
  <span class="loading loading-spinner"></span>
{/if}
