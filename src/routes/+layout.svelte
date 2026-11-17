<script lang="ts">
  import '../app.css';
  import '$lib/keyboard';
  import { page } from '$app/state';
  import { appPath } from '$lib/navigation';
  import PostMenu from '$lib/components/PostMenu.svelte';
  import PostViewer from '$lib/components/PostViewer.svelte';
  import PullToRefresh from '$lib/components/PullToRefresh.svelte';
  import SideMenu from '$lib/components/mobile/SideMenu.svelte';
  import TopBar from '$lib/components/mobile/TopBar.svelte';
  import SidePanels from '$lib/components/sidebar/SidePanels.svelte';
  import TagMenu from '$lib/components/TagMenu.svelte';
  import Toast from '$lib/components/Toast.svelte';
  import { library } from '$lib/state/library.svelte';
  import { rememberSearch, staged, urlQuery } from '$lib/state/search.svelte';
  import { scrollPage } from '$lib/pageScroller.svelte';
  import { rememberScroll } from '$lib/scrollMemory';
  import { drawerLayout, linuxDesktop } from '$lib/screen';
  import { settings } from '$lib/state/settings.svelte';
  import { applyTheme } from '$lib/theme';
  import { onMount, untrack } from 'svelte';

  let { children } = $props();

  const REFRESHABLE = new Set(['/', '/for-you', '/likes', '/disliked']);

  rememberScroll();
  if (linuxDesktop) document.documentElement.dataset.os = 'linux';

  const query = $derived(urlQuery(page.url));
  const fullScreen = $derived(appPath() === '/feed');

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
  });

  // the engine has started and the app is up, so the plain HTML loading screen from app.html can fade away
  onMount(() => {
    const splash = document.getElementById('splash');
    if (!splash) return;
    splash.classList.add('leaving');
    setTimeout(() => splash.remove(), 300);
  });

  // a new version replaces the old service worker right away, so reload once onto its files.
  // the very first install also takes control of an uncontrolled page, which is not an update and needs no reload
  onMount(() => {
    if (!('serviceWorker' in navigator)) return;
    let controlled = !!navigator.serviceWorker.controller;
    let reloading = false;
    navigator.serviceWorker.addEventListener('controllerchange', () => {
      if (!controlled) {
        controlled = true;
        return;
      }
      if (reloading) return;
      reloading = true;
      location.reload();
    });
  });

  // an installed app can stay open for days, so look for a new version whenever it comes back into view
  function checkForUpdate() {
    if (document.visibilityState !== 'visible' || !('serviceWorker' in navigator)) return;
    navigator.serviceWorker.getRegistration().then((registration) => registration?.update().catch(() => {}));
  }

  function oncontextmenu(e: MouseEvent) {
    if (e.target instanceof Element && e.target.closest('img, video')) e.preventDefault();
  }
</script>

<svelte:document {oncontextmenu} onvisibilitychange={checkForUpdate} />

{#if drawerLayout.current}
  <TopBar />
  <SideMenu />
  {#if REFRESHABLE.has(appPath())}<PullToRefresh />{/if}
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
  {#if !drawerLayout.current}
    <div class="fixed right-[calc(--spacing(2)+var(--page-scrollbar))] bottom-2 z-(--z-floating) flex flex-col gap-2">
      <button class="btn btn-circle shadow" title="Go to top" onclick={() => scrollPage('top')}>↑</button>
      <button class="btn btn-circle shadow" title="Go to bottom" onclick={() => scrollPage('bottom')}>↓</button>
    </div>
  {/if}
{/if}

<PostViewer />
<TagMenu />
<PostMenu />
<Toast />
