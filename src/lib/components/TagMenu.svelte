<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { appPath, hrefWith } from '$lib/navigation';
  import { library } from '$lib/state/library.svelte';
  import { ARTISTS_PAGE, searchHref } from '$lib/state/search.svelte';
  import { tagMenu } from '$lib/state/tagMenu.svelte';
  import PopoverMenu from './PopoverMenu.svelte';

  const open = $derived(tagMenu.current);
  const inSearch = $derived(open ? open.target.state(open.name) : null);

  // the Artists page searching for artists who draw the tag; already there, its post filter stays and the viewer closes
  function artistSearchHref(name: string) {
    if (appPath() === ARTISTS_PAGE) return hrefWith(page.url, { q: name, view: null, post: null, site: null });
    return searchHref(ARTISTS_PAGE, name);
  }

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
    <li><button onclick={() => run(() => goto(artistSearchHref(name)))}>🎨 Artist search</button></li>
  {/if}
</PopoverMenu>
