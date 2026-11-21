<script lang="ts">
  import { goto } from '$app/navigation';
  import { appPath } from '$lib/navigation';
  import { library } from '$lib/state/library.svelte';
  import { ARTISTS_PAGE, artistSearch } from '$lib/state/search.svelte';
  import { tagMenu } from '$lib/state/tagMenu.svelte';
  import PopoverMenu from './PopoverMenu.svelte';

  const open = $derived(tagMenu.current);
  const inSearch = $derived(open ? open.target.state(open.name) : null);
  // the Artists page has a second search that picks the artists shown
  const onArtistsPage = $derived(appPath() === ARTISTS_PAGE);
  const inArtistSearch = $derived(open ? artistSearch.state(open.name) : null);

  function run(action: () => unknown) {
    action();
    tagMenu.close();
  }
</script>

<PopoverMenu anchor={open?.anchor ?? null} class="menu-md w-max" onclose={() => tagMenu.close()}>
  {#if open}
    {@const { name, type, target, actions } = open}
    {#each actions as extra (extra.label)}
      <li><button onclick={() => run(extra.action)}>{extra.label}</button></li>
    {/each}
    {#if inSearch !== 'include'}
      <li><button onclick={() => run(() => target.add(name, type))}>➕ Include in search</button></li>
    {/if}
    {#if inSearch !== 'exclude'}
      <li><button onclick={() => run(() => target.add(`-${name}`, type))}>➖ Exclude from search</button></li>
    {/if}
    {#if inSearch}
      <li><button onclick={() => run(() => target.remove(name))}>❌ Remove from search</button></li>
    {/if}
    {#if onArtistsPage}
      {#if inArtistSearch}
        <li><button onclick={() => run(() => artistSearch.remove(name))}>❌ Remove from artist search</button></li>
      {:else}
        <li><button onclick={() => run(() => artistSearch.add(name, type))}>🎨 Add to artist search</button></li>
      {/if}
    {/if}
    <li>
      <button onclick={() => run(() => library.toggleTag('favouriteTags', name))}>
        {library.starred.has(name) ? '✖️ Unfavourite tag' : '⭐ Favourite tag'}
      </button>
    </li>
    {#if type === 'artist'}
      <li>
        <button onclick={() => run(() => library.toggleTag('followedArtists', name))}>
          {library.followed.has(name) ? '👋 Unfollow artist' : '👥 Follow artist'}
        </button>
      </li>
    {/if}
    <li>
      <button onclick={() => run(() => library.toggleTag('blacklist', name))}>
        {library.blacklisted.has(name) ? '✅ Remove from blacklist' : '🚫 Blacklist tag (never show)'}
      </button>
    </li>
    <li><button onclick={() => run(() => goto(target.searchHref(name)))}>🔍 Search this tag</button></li>
  {/if}
</PopoverMenu>
