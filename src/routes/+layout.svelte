<script lang="ts">
  import '../app.css';
  import '$lib/keyboard';
  import { page } from '$app/state';
  import { appPath } from '$lib/navigation';
  import PostMenu from '$lib/components/PostMenu.svelte';
  import PostViewer from '$lib/components/PostViewer.svelte';
  import PullToSearch from '$lib/components/PullToSearch.svelte';
  import BottomBar from '$lib/components/mobile/BottomBar.svelte';
  import TopBar from '$lib/components/mobile/TopBar.svelte';
  import SidePanels from '$lib/components/sidebar/SidePanels.svelte';
  import TagMenu from '$lib/components/TagMenu.svelte';
  import Toast from '$lib/components/Toast.svelte';
  import UpdateNotice from '$lib/components/UpdateNotice.svelte';
  import { library } from '$lib/state/library.svelte';
  import { rememberSearch, staged, urlQuery } from '$lib/state/search.svelte';
  import { scrollPage } from '$lib/pageScroller.svelte';
  import { rememberScroll } from '$lib/scrollMemory';
  import { drawerLayout, linuxDesktop } from '$lib/screen';
  import { settings } from '$lib/state/settings.svelte';
  import { applyTheme } from '$lib/theme';
  import { listen } from '@tauri-apps/api/event';
  import type { SyncStatus } from '$lib/api';
  import { onMount, tick, untrack } from 'svelte';
  import { fade } from 'svelte/transition';

  let { children } = $props();

  const PULL_PAGES = new Set(['/', '/for-you', '/artists', '/likes', '/disliked']);
  const SCROLL_EDGE_PX = 8;

  let atTop = $state(true);
  let atBottom = $state(true);

  rememberScroll();
  if (linuxDesktop) document.documentElement.dataset.os = 'linux';

  const query = $derived(urlQuery(page.url));
  const fullScreen = $derived(appPath() === '/tiktok');

  $effect(() => applyTheme(settings.theme));
  $effect(() => {
    document.documentElement.dataset.layout = drawerLayout.current ? 'drawer' : 'sidebar';
  });

  $effect(() => {
    const ran = page.url.searchParams.get('q');
    if (ran !== null) rememberSearch(appPath(), ran);
  });

  $effect(() => {
    const current = query;
    untrack(() => staged.load(current));
  });

  onMount(() => {
    library.load();
    // another device's tag, blacklist or saved search changes just arrived
    const unlisten = listen<SyncStatus>('sync', (event) => {
      if (!event.payload.running && event.payload.pulled > 0) library.load();
    });
    return () => unlisten.then((stop) => stop());
  });

  // the engine has started and the app is up, so the plain HTML loading screen from app.html can fade away
  onMount(() => {
    const splash = document.getElementById('splash');
    if (!splash) return;
    splash.classList.add('leaving');
    setTimeout(() => splash.remove(), 300);
  });

  onMount(() => {
    // capture scroll from the page and from ScrollTable's inner scroller
    const update = (event: Event) => {
      const target = event.target;
      // sideways rows have no vertical overflow and would read as both top and bottom
      if (target instanceof HTMLElement && target !== document.documentElement && target.scrollHeight <= target.clientHeight + SCROLL_EDGE_PX) return;
      measure(target instanceof HTMLElement && target !== document.documentElement ? target : document.documentElement);
    };
    document.addEventListener('scroll', update, { capture: true, passive: true });
    measure(document.documentElement);
    return () => document.removeEventListener('scroll', update, { capture: true });
  });

  function measure(element: HTMLElement) {
    atTop = element.scrollTop <= SCROLL_EDGE_PX;
    atBottom = element.scrollTop + element.clientHeight >= element.scrollHeight - SCROLL_EDGE_PX;
  }

  $effect(() => {
    page.url;
    // wait for the new page to render so its height is known
    untrack(() => tick().then(() => requestAnimationFrame(() => measure(document.documentElement))));
  });

  function oncontextmenu(e: MouseEvent) {
    if (e.target instanceof Element && e.target.closest('img, video')) e.preventDefault();
  }
</script>

<svelte:document {oncontextmenu} />

{#if drawerLayout.current}
  <TopBar />
  <BottomBar />
  {#if PULL_PAGES.has(appPath())}<PullToSearch />{/if}
{/if}

{#if fullScreen}
  {@render children()}
{:else}
  <div class="flex min-h-dvh pt-(--top-bar)">
    <SidePanels />
    <main class="relative min-w-0 flex-1 pr-(--page-scrollbar) pt-1 pb-(--bottom-bar) pl-2">
      {@render children()}
    </main>
  </div>
  <div class="pointer-events-none fixed inset-x-0 top-(--top-bar-offset) bottom-(--bottom-bar) z-(--z-floating) transition-[top] duration-150">
    {#if !atTop}
      <button
        class="btn btn-sm pointer-events-auto absolute top-2 left-1/2 -translate-x-1/2"
        title="Go to top"
        aria-label="Go to top"
        transition:fade={{ duration: 75 }}
        onclick={() => scrollPage('top')}>⬆️</button
      >
    {/if}
    {#if !atBottom}
      <button
        class="btn btn-sm pointer-events-auto absolute bottom-2 left-1/2 -translate-x-1/2"
        title="Go to bottom"
        aria-label="Go to bottom"
        transition:fade={{ duration: 75 }}
        onclick={() => scrollPage('bottom')}>⬇️</button
      >
    {/if}
  </div>
{/if}

<PostViewer />
<TagMenu />
<PostMenu />
<Toast />
<UpdateNotice />
