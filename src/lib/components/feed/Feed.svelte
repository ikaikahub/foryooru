<script module lang="ts">
  type Kept = { posts: PostDetail[]; exhausted: boolean; current: number; since: number; fetchChips: string[] };

  const kept = new Map<string, Kept>();
</script>

<script lang="ts">
  import { goto, pushState, replaceState } from '$app/navigation';
  import { page } from '$app/state';
  import { api, errorMessage, type FeedOptions, type PostDetail } from '$lib/api';
  import { PRELOAD_AHEAD, touchDrag } from '$lib/attachments';
  import { addImpression, flushImpressions } from '$lib/impressions';
  import { isShortcut } from '$lib/keys';
  import { closeLayer, hrefWith, openLayer } from '$lib/navigation';
  import { signal, flushSignals } from '$lib/signals';
  import { library } from '$lib/state/library.svelte';
  import { FLICK_SPEED, SPRING, swipeAxis, swipeSpeed, trackMove, type Moves } from '$lib/motion';
  import { onRerun } from '$lib/state/rerun.svelte';
  import { searchHref, staged, type TagTarget } from '$lib/state/search.svelte';
  import { drawerLayout, linuxDesktop } from '$lib/screen';
  import { settings } from '$lib/state/settings.svelte';
  import { tagMenu } from '$lib/state/tagMenu.svelte';
  import { toast } from '$lib/state/toast.svelte';
  import { viewer } from '$lib/state/viewer.svelte';
  import { onMount, tick, untrack } from 'svelte';
  import type { Attachment } from 'svelte/attachments';
  import { Spring } from 'svelte/motion';
  import { SvelteSet } from 'svelte/reactivity';
  import ErrorNote from '../ErrorNote.svelte';
  import AlgorithmControls from '../AlgorithmControls.svelte';
  import InfoSheet from '../InfoSheet.svelte';
  import LoadingBar from '../LoadingBar.svelte';
  import PostTags from '../PostTags.svelte';
  import PullIndicator, { PULL_THRESHOLD, pullDistance } from '../PullIndicator.svelte';
  import SidePanels from '../sidebar/SidePanels.svelte';
  import FeedItem from './FeedItem.svelte';

  let {
    chips,
    initialPost = null,
    initialSite = null,
  }: { chips: string[]; initialPost?: number | null; initialSite?: string | null } = $props();

  const CURRENT_AT = 0.6;
  const SETTLED_AT = 0.95;
  const LOAD_WHEN_LEFT = 10;
  const PAGE_MS = 220;
  const PAGE_DRAG_SHARE = 0.2;
  const SETTLE_MS = 150;

  const chosenOptions = (): FeedOptions => ({
    hideLiked: settings.hideLiked,
    similarPerArtist: settings.similarPerArtist,
    discover: settings.feedDiscover,
  });
  const keyFor = (options: FeedOptions) =>
    `${chips.join(' ')}\n${options.hideLiked}\n${options.similarPerArtist}\n${options.discover}`;

  let options = untrack(chosenOptions);
  let key = keyFor(options);
  const saved = kept.get(key);
  let since = saved?.since ?? Date.now();
  const sidebarWidth = $derived(drawerLayout.current || settings.sidebarCollapsed ? 0 : settings.sidebarWidth);
  let posts = $state<PostDetail[]>(saved?.posts ?? []);
  let exhausted = $state(saved?.exhausted ?? false);
  let fetchChips = saved?.fetchChips ?? untrack(() => chips);
  let loading = $state(false);
  let error = $state<unknown>(null);
  let current = $state(saved?.current ?? 0);
  let scroller = $state<HTMLElement>();
  let sheetHeight = $state(0);
  const info = new Spring(0, SPRING);
  const pull = new Spring(0, SPRING);

  const target: TagTarget = {
    state: (name) => staged.state(name),
    add: (tag, type) => staged.add(tag, type),
    remove: (name) => staged.remove(name),
    searchHref: (name) => searchHref('/tiktok', staged.ratingQuery(name)),
  };

  async function loadNext() {
    if (loading || exhausted) return;
    loading = true;
    const requested = options;
    try {
      await flushImpressions();
      await flushSignals();
      const batch = await api.feedNext(fetchChips, posts.map((post) => post.id), since, requested);
      if (requested === options) {
        library.remember(batch.posts);
        posts = [...posts, ...batch.posts];
        exhausted = batch.exhausted;
        if (batch.broadened) {
          if (fetchChips === chips) toast.show('Showing broader results…');
          fetchChips = batch.broadened;
        }
        updateLink();
      }
    } catch (caught) {
      if (requested === options) error = caught;
    } finally {
      loading = false;
    }
    if (requested !== options) loadNext();
  }

  function updateLink() {
    const post = posts[current];
    if (post) replaceState(hrefWith(page.url, { site: settings.site, post: post.id }), page.state);
  }

  async function startAt(id: number) {
    const existing = posts.findIndex((post) => post.id === id);
    if (existing !== -1) {
      current = existing;
      await tick();
      scrollToIndex(existing);
      updateLink();
      return;
    }
    try {
      const post = await api.openPost(id);
      library.remember([post]);
      posts = [post];
      current = 0;
      exhausted = false;
      error = null;
      updateLink();
      await tick();
      scrollToIndex(0);
      loadNext();
    } catch (caught) {
      toast.show(`Couldn't open that post: ${errorMessage(caught)}`);
      if (saved) tick().then(() => scrollToIndex(current));
      else loadNext();
    }
  }

  $effect(() => {
    const chosen = chosenOptions();
    untrack(() => {
      const chosenKey = keyFor(chosen);
      if (chosenKey === key) return;
      options = chosen;
      key = chosenKey;
      posts = posts.slice(0, current + 1);
      exhausted = false;
      fetchChips = chips;
      error = null;
      loadNext();
    });
  });

  let settledSince: number | null = null;
  let settled = false;
  let shownMs = 0;
  const loadedMedia = new SvelteSet<number>();
  const scrolledBackTo = new Set<number>();
  const mediaReady = (post: PostDetail) => post.media === 'image' || loadedMedia.has(post.id);
  const currentReady = $derived(posts[current] === undefined || mediaReady(posts[current]));
  const preloadStarted = new SvelteSet<number>();
  const inPreloadWindow = (index: number) => index > current && index <= current + PRELOAD_AHEAD;

  $effect(() => {
    if (!currentReady) return;
    for (const post of posts.slice(current + 1, current + 1 + PRELOAD_AHEAD)) preloadStarted.add(post.id);
  });

  function pauseClock() {
    if (settledSince !== null) shownMs += performance.now() - settledSince;
    settledSince = null;
  }

  function updateClock() {
    const post = posts[current];
    const showing = settled && !document.hidden && post !== undefined && mediaReady(post);
    if (!showing) pauseClock();
    else settledSince ??= performance.now();
  }

  function noteLoaded(id: number) {
    loadedMedia.add(id);
    if (posts[current]?.id === id) updateClock();
  }

  function recordDwell() {
    const post = posts[current];
    if (!post) return;
    pauseClock();
    addImpression({ post: post.id, source: 'feed', ms: Math.round(shownMs) });
    shownMs = 0;
  }

  function setCurrent(index: number) {
    if (index === current) return;
    recordDwell();
    if (index < current && posts[index] && !scrolledBackTo.has(posts[index].id)) {
      scrolledBackTo.add(posts[index].id);
      signal(posts[index].id, 'scrollBack');
    }
    current = index;
    updateLink();
    settled = false;
    if (posts.length - index <= LOAD_WHEN_LEFT) loadNext();
  }

  async function showRelated(id: number) {
    await closeLayer('feedInfo');
    signal(id, 'opened');
    // the related post gets its own history entry, so back returns to the post it was opened from
    const remember = () => pushState(hrefWith(page.url, { site: settings.site, post: id }), page.state);
    const existing = posts.findIndex((post) => post.id === id);
    if (existing !== -1) {
      remember();
      scrollToIndex(existing);
      return;
    }
    try {
      const related = await api.postDetail(id);
      library.remember([related]);
      posts = [...posts.slice(0, current + 1), related, ...posts.slice(current + 1)];
      await tick();
      remember();
      scrollToIndex(current + 1);
    } catch (caught) {
      toast.show(`Couldn't open that post: ${errorMessage(caught)}`);
    }
  }

  // going back to an entry left by showRelated scrolls to the post it names
  function onpopstate() {
    const linked = Number(new URL(location.href).searchParams.get('post'));
    const index = posts.findIndex((post) => post.id === linked);
    if (index !== -1 && index !== current) scrollToIndex(index);
  }

  // the info sheet is a history layer, so Back closes it before leaving TikTok
  $effect(() => {
    info.target = page.state.feedInfo ? 1 : 0;
  });

  function toggleInfo() {
    if (page.state.feedInfo) {
      closeLayer('feedInfo');
      return;
    }
    if (posts[current]) signal(posts[current].id, 'info');
    openLayer('feedInfo');
  }

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        const index = Number((entry.target as HTMLElement).dataset.index);
        if (entry.intersectionRatio >= CURRENT_AT) setCurrent(index);
        if (index !== current) continue;
        settled = entry.intersectionRatio >= SETTLED_AT;
        updateClock();
      }
    },
    { threshold: [CURRENT_AT, SETTLED_AT] },
  );

  const watch =
    (index: number): Attachment =>
    (element) => {
      (element as HTMLElement).dataset.index = String(index);
      observer.observe(element);
      return () => observer.unobserve(element);
    };

  $effect(() => {
    kept.set(key, { posts, exhausted, current, since, fetchChips });
  });

  onRerun(() => {
    recordDwell();
    options = { ...options };
    since = Date.now();
    posts = [];
    current = 0;
    settled = false;
    exhausted = false;
    fetchChips = chips;
    error = null;
    scroller?.scrollTo({ top: 0, behavior: 'instant' });
    loadNext();
  });

  onMount(() => {
    if (initialPost !== null && (initialSite === null || initialSite === settings.site)) {
      void startAt(initialPost);
    } else if (saved) {
      tick().then(() => {
        scrollToIndex(current);
        updateLink();
      });
    } else {
      loadNext();
    }
    return () => {
      recordDwell();
      flushImpressions();
      flushSignals();
      observer.disconnect();
      clearTimeout(settleTimer);
      stopAnimation();
    };
  });

  function scrollToIndex(index: number) {
    const item = scroller?.querySelectorAll('section')[index];
    item?.scrollIntoView({ block: 'start', behavior: 'instant' });
  }

  function openSearch() {
    openLayer('searching');
  }

  let animation = 0;
  let animating = $state(false);

  function stopAnimation() {
    cancelAnimationFrame(animation);
    animating = false;
  }

  function animateTo(top: number) {
    if (!scroller) return;
    const element = scroller;
    stopAnimation();
    animating = true;
    const from = element.scrollTop;
    const start = performance.now();
    const frame = (now: number) => {
      const progress = Math.min(1, (now - start) / PAGE_MS);
      element.scrollTop = from + (top - from) * (1 - (1 - progress) ** 3);
      if (progress < 1) animation = requestAnimationFrame(frame);
      else animating = false;
    };
    animation = requestAnimationFrame(frame);
  }

  let touchPaging = $state(false);
  let paging: { x0: number; y0: number; top0: number; mode: 'page' | 'none' | null; moves: Moves } | null = null;
  const nearestTop = (top: number) => (scroller ? Math.round(top / scroller.clientHeight) * scroller.clientHeight : 0);

  let settleTimer: ReturnType<typeof setTimeout> | undefined;
  function onscroll() {
    clearTimeout(settleTimer);
    settleTimer = setTimeout(settle, SETTLE_MS);
  }

  function settle() {
    if (!scroller || paging || animating) return;
    const target = nearestTop(scroller.scrollTop);
    if (Math.abs(scroller.scrollTop - target) > 1) animateTo(target);
  }

  function ontouchstart(e: TouchEvent) {
    if (e.touches.length !== 1) {
      if (paging?.mode === 'page') animateTo(nearestTop(paging.top0));
      paging = null;
      return;
    }
    stopAnimation();
    touchPaging = true;
    const touch = e.touches[0];
    paging = { x0: touch.clientX, y0: touch.clientY, top0: nearestTop(scroller?.scrollTop ?? 0), mode: null, moves: [] };
  }

  function ontouchmove(e: TouchEvent) {
    if (!paging || !scroller) return;
    const touch = e.touches[0];
    const dx = touch.clientX - paging.x0;
    const dy = touch.clientY - paging.y0;
    if (paging.mode === null) {
      const axis = swipeAxis(dx, dy);
      if (axis) paging.mode = axis === 'y' ? 'page' : 'none';
    }
    if (paging.mode !== 'page') return;
    e.preventDefault();
    paging.moves = trackMove(paging.moves, e.timeStamp, touch.clientY);
    if (paging.top0 === 0 && dy > 0) {
      pull.set(pullDistance(dy), { instant: true });
      return;
    }
    pull.set(0, { instant: true });
    scroller.scrollTop = paging.top0 - dy;
  }

  function ontouchend() {
    const swipe = paging;
    paging = null;
    if (!swipe || swipe.mode !== 'page' || !scroller) return;
    if (pull.current > 0) {
      if (pull.current >= PULL_THRESHOLD) openLayer('searching');
      pull.target = 0;
      return;
    }
    const height = scroller.clientHeight;
    const moved = scroller.scrollTop - swipe.top0;
    const speed = swipeSpeed(swipe.moves);
    let index = Math.round(swipe.top0 / height);
    if (speed < -FLICK_SPEED || moved > height * PAGE_DRAG_SHARE) index += 1;
    else if (speed > FLICK_SPEED || moved < -height * PAGE_DRAG_SHARE) index -= 1;
    animateTo(Math.max(0, Math.min(posts.length, index)) * height);
  }

  function close() {
    if (history.length > 1) history.back();
    else goto(searchHref('/for-you', chips.join(' ')));
  }

  function onkeydown(e: KeyboardEvent) {
    if (viewer.current !== null || !isShortcut(e)) return;
    const post = posts[current];
    if (['ArrowDown', 'j', 'PageDown'].includes(e.key)) scrollToIndex(current + 1);
    else if (['ArrowUp', 'k', 'PageUp'].includes(e.key)) scrollToIndex(current - 1);
    else if (e.key === 'f' && post) library.toggleLike(post.id);
    else if (e.key === 'd' && post) library.toggleDislike(post.id);
    else if (e.key === 'i' && drawerLayout.current) toggleInfo();
    else if (e.key === 'i') settings.sidebarCollapsed = !settings.sidebarCollapsed;
    else if (e.key === 'Escape') {
      if (tagMenu.current) tagMenu.close();
      else if (page.state.feedInfo) closeLayer('feedInfo');
      else close();
    } else return;
    e.preventDefault();
  }
</script>

<svelte:window {onkeydown} {onpopstate} />
<svelte:document onvisibilitychange={updateClock} />

{#snippet currentTags()}
  {#if posts[current]}<PostTags post={posts[current]} {target} onrelated={showRelated} />{/if}
{/snippet}

<SidePanels children={currentTags} />

<div class="contents" data-theme="dracula">
  <div
    bind:this={scroller}
    class={[
      'fixed inset-x-0 top-(--top-bar) bottom-(--bottom-bar) z-(--z-feed) touch-none overflow-y-auto overscroll-contain no-scrollbar bg-black text-white',
      !touchPaging && !animating && 'snap-y snap-mandatory',
    ]}
    style:padding-left="calc(var(--rail-width) + {sidebarWidth}px)"
    role="feed"
    aria-busy={loading}
    {@attach touchDrag({ start: ontouchstart, move: ontouchmove, end: ontouchend })}
    onscroll={linuxDesktop ? onscroll : undefined}
  >
    {#each posts as post, index (post.id)}
      <FeedItem
        {post}
        current={index === current}
        preload={inPreloadWindow(index) && preloadStarted.has(post.id)}
        loaded={mediaReady(post)}
        watch={watch(index)}
        onloaded={() => noteLoaded(post.id)}
        ontags={drawerLayout.current ? toggleInfo : undefined}
        onsearch={drawerLayout.current ? openSearch : undefined}
        {target}
      />
    {/each}
    {#if loading && posts.length}
      <section class="flex h-full snap-start items-center justify-center">
        <span class="loading loading-spinner loading-lg"></span>
      </section>
    {/if}
    {#if exhausted || error}
      <section class="flex h-full snap-start items-center justify-center p-4 text-center opacity-80">
        {#if error}
          <ErrorNote {error} />
        {:else}
          <p class="max-w-sm">
            {posts.length ? "That's everything that matches right now." : 'Nothing matches these filters yet.'} More posts might be fetched in the
            background.
          </p>
        {/if}
      </section>
    {/if}
  </div>

  <PullIndicator distance={pull.current} />

  {#if !drawerLayout.current}
    <div class="fixed top-2 z-(--z-feed-overlay) flex gap-2" style:left="calc(var(--rail-width) + {sidebarWidth}px + 0.5rem)">
      <AlgorithmControls page="feed" idleClass="border-white/25 bg-black/60 text-white" />
    </div>
  {/if}

  {#if drawerLayout.current}
    {#if info.current > 0}
      <button class="fixed inset-0 z-(--z-drawer-backdrop) cursor-default bg-black" style:opacity={0.4 * info.current} aria-label="Close info" onclick={() => closeLayer('feedInfo')}
      ></button>
    {/if}
    <div class="fixed inset-x-0 bottom-(--bottom-bar) z-(--z-drawer) h-[65%]" style:transform="translateY({(1 - info.current) * 100}%)" bind:clientHeight={sheetHeight}>
      {#if info.current > 0}
        <InfoSheet
          post={posts[current] ?? null}
          {target}
          actions={false}
          onrelated={showRelated}
          ondrag={(dy) => info.set(Math.max(0, 1 - dy / Math.max(1, sheetHeight)), { instant: true })}
          onrelease={(speed) => (speed > FLICK_SPEED || info.current < 0.5 ? closeLayer('feedInfo') : (info.target = 1))}
        />
      {/if}
    </div>
  {/if}

  {#if loading && !posts.length}<LoadingBar text="Finding posts…" />{/if}
</div>
