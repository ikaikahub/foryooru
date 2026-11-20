<script lang="ts">
  import { api, imageUrl, type PostDetail } from '$lib/api';
  import { library } from '$lib/state/library.svelte';
  import { staged } from '$lib/state/search.svelte';
  import { tagMenu } from '$lib/state/tagMenu.svelte';
  import { viewer } from '$lib/state/viewer.svelte';
  import { settings } from '$lib/state/settings.svelte';
  import { isShortcut } from '$lib/keys';
  import { taps } from '$lib/attachments';
  import { isZoomed, justZoomed, resetZoom } from '$lib/zoom';
  import { untrack } from 'svelte';
  import { FLICK_SPEED, SPRING, swipeAxis, swipeSpeed, trackMove, type Moves } from '$lib/motion';
  import { Spring } from 'svelte/motion';
  import { SvelteMap } from 'svelte/reactivity';
  import { drawerLayout } from '$lib/screen';
  import ErrorNote from './ErrorNote.svelte';
  import LikeBurst from './LikeBurst.svelte';
  import PostMedia from './PostMedia.svelte';
  import PostTags from './PostTags.svelte';
  import ViewerActions from './ViewerActions.svelte';
  import InfoSheet from './InfoSheet.svelte';

  const BAR_BUTTON = 'btn btn-sm';
  const SLIDE_GAP = 16;
  const SWIPE_SHARE = 0.2;
  const CLICK_AFTER_SWIPE_MS = 250;
  const BACKDROP_OPACITY = 0.85;
  const VIDEO_CONTROLS_PX = 48;
  const SHEET_SHARE = 0.65;
  const DISMISS_SHARE = 0.2;
  const STRETCH_MAX = 0.08;

  type Mode = 'immersive' | 'normal' | 'info';

  const details = new SvelteMap<number, PostDetail>();
  const failures = new SvelteMap<number, unknown>();
  const pending = new Set<number>();
  const slideOffset = new Spring(0, SPRING);
  const stretch = new Spring(0, SPRING);
  const infoProgress = new Spring(0, SPRING);
  const dismissOffset = new Spring(0, SPRING);
  let tagsOpen = $state(false);
  let figure = $state<HTMLElement>();
  let pendingStep: number | null = null;
  let swipedAt = 0;
  let likeBurst = $state<{ post: number; count: number } | null>(null);
  let viewerHeight = $state(0);
  let mode = $state<Mode>('normal');

  const mobile = $derived(drawerLayout.current);
  const progress = $derived(infoProgress.current);
  const barsShown = $derived(mode !== 'immersive' && dismissOffset.target === 0);
  const sheetPx = $derived(viewerHeight * SHEET_SHARE);

  function likeFromTap() {
    const id = viewer.current;
    if (id === null) return;
    library.like(id);
    likeBurst = { post: id, count: likeBurst?.post === id ? likeBurst.count + 1 : 1 };
  }

  let drag: {
    pointer: number;
    x0: number;
    y0: number;
    dx: number;
    axis: 'x' | 'y' | null;
    startMode: Mode;
    info0: number;
    moves: Moves;
  } | null = null;

  const post = $derived(viewer.current === null ? null : (details.get(viewer.current) ?? null));
  let lastPost = $state<PostDetail | null>(null);
  const tagsPost = $derived(post ?? lastPost);

  $effect(() => {
    if (post || viewer.current === null) lastPost = post;
  });
  const neighbour = (delta: number) => (viewer.index < 0 ? undefined : viewer.ids[viewer.index + delta]);
  const slides = $derived.by(() => {
    if (viewer.current === null) return [];
    const around = [-1, 1].flatMap((delta) => {
      const id = neighbour(delta);
      return id === undefined ? [] : [{ delta, id }];
    });
    return [...around, { delta: 0, id: viewer.current }];
  });

  $effect(() => {
    if (viewer.current === null || !figure) return;
    for (const img of figure.querySelectorAll<HTMLImageElement>('[data-delta]:not([data-delta="0"]) img')) resetZoom(img);
  });

  $effect(() => {
    if (!viewer.shown) return;
    untrack(() => {
      mode = 'normal';
      likeBurst = null;
      for (const spring of [slideOffset, stretch, infoProgress, dismissOffset]) spring.set(0, { instant: true });
    });
    const root = document.documentElement;
    const scrolled = root.scrollTop;
    const openedAt = location.href;
    root.classList.add('overflow-hidden');
    return () => {
      root.classList.remove('overflow-hidden');
      root.scrollTop = location.href === openedAt ? scrolled : 0;
    };
  });

  async function load(id: number) {
    if (details.has(id) || pending.has(id)) return;
    pending.add(id);
    failures.delete(id);
    try {
      const found = await api.postDetail(id);
      library.remember([found]);
      details.set(id, found);
    } catch (error) {
      failures.set(id, error);
    } finally {
      pending.delete(id);
    }
  }

  $effect(() => {
    const id = viewer.current;
    untrack(() => {
      if (id === null) {
        details.clear();
        failures.clear();
        return;
      }
      load(id);
      for (const neighbour of [viewer.ids[viewer.index - 1], viewer.ids[viewer.index + 1]]) {
        if (neighbour === undefined) continue;
        load(neighbour);
        new Image().src = imageUrl('sample', neighbour);
      }
    });
  });

  function onMediaClick(e: MouseEvent) {
    if (mobile || performance.now() - swipedAt < CLICK_AFTER_SWIPE_MS || justZoomed()) return;
    if (e.target instanceof Element && !e.target.closest('img, video, button')) viewer.close();
  }

  const ignoreInterrupted = () => {};

  function finishSlide() {
    if (pendingStep === null) return;
    const delta = pendingStep;
    pendingStep = null;
    viewer.step(delta);
    slideOffset.set(0, { instant: true });
  }

  function slide(delta: number) {
    finishSlide();
    stretch.target = 0;
    if (delta !== 0 && neighbour(delta) !== undefined && figure) {
      pendingStep = delta;
      slideOffset.set(-delta * (figure.clientWidth + SLIDE_GAP)).then(finishSlide, ignoreInterrupted);
      return;
    }
    slideOffset.target = 0;
    if (delta !== 0) viewer.step(delta);
  }

  function setMode(next: Mode) {
    mode = next;
    infoProgress.target = next === 'info' ? 1 : 0;
  }

  function toggleBars() {
    setMode(mode === 'immersive' ? 'normal' : mode === 'info' ? 'normal' : 'immersive');
  }

  function toggleInfo() {
    setMode(mode === 'info' ? 'normal' : 'info');
  }

  function dragInfo(from: number, dy: number) {
    infoProgress.set(Math.min(1, Math.max(0, from - dy / sheetPx)), { instant: true });
  }

  function releaseInfo(startMode: Mode, speed: number) {
    const opens = speed < -FLICK_SPEED || (speed <= FLICK_SPEED && infoProgress.current > 0.5);
    setMode(opens ? 'info' : startMode === 'info' ? 'normal' : startMode);
  }

  function releaseDismiss(speed: number) {
    if (dismissOffset.current > viewerHeight * DISMISS_SHARE || speed > FLICK_SPEED) {
      dismissOffset.set(viewerHeight).then(() => viewer.close(), ignoreInterrupted);
    } else dismissOffset.target = 0;
  }

  function onVideoControls(target: EventTarget | null, y: number) {
    return target instanceof HTMLVideoElement && target.getBoundingClientRect().bottom - y < VIDEO_CONTROLS_PX;
  }

  function onpointerdown(e: PointerEvent) {
    const touched = e.target instanceof Element ? e.target.closest('img') : null;
    const blocked = (touched && isZoomed(touched)) || onVideoControls(e.target, e.clientY);
    if ((tagsOpen && !mobile) || !e.isPrimary || e.button !== 0 || blocked) {
      if (drag) {
        slideOffset.target = 0;
        stretch.target = 0;
      }
      drag = null;
      return;
    }
    finishSlide();
    drag = { pointer: e.pointerId, x0: e.clientX, y0: e.clientY, dx: 0, axis: null, startMode: mode, info0: infoProgress.current, moves: [] };
  }

  function onpointermove(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.pointer || !figure) return;
    const [dx, dy] = [e.clientX - drag.x0, e.clientY - drag.y0];
    if (drag.axis === null) {
      drag.axis = swipeAxis(dx, dy);
      if (drag.axis === 'x' || (drag.axis === 'y' && mobile)) figure.setPointerCapture(e.pointerId);
    }
    if (drag.axis === 'x') {
      drag.dx = dx;
      if (neighbour(dx < 0 ? 1 : -1) === undefined) {
        slideOffset.set(0, { instant: true });
        stretch.set(Math.sign(dx) * STRETCH_MAX * (1 - Math.exp(-Math.abs(dx) / figure.clientWidth)), { instant: true });
      } else {
        stretch.set(0, { instant: true });
        slideOffset.set(dx, { instant: true });
      }
      drag.moves = trackMove(drag.moves, e.timeStamp, e.clientX);
    } else if (drag.axis === 'y' && mobile) {
      if (drag.startMode === 'info' || dy < 0) {
        dismissOffset.set(0, { instant: true });
        dragInfo(drag.info0, dy);
      } else {
        infoProgress.set(drag.info0, { instant: true });
        dismissOffset.set(dy, { instant: true });
      }
      drag.moves = trackMove(drag.moves, e.timeStamp, e.clientY);
    }
  }

  function onpointerup(e: PointerEvent) {
    const swipe = drag;
    if (!swipe || e.pointerId !== swipe.pointer) return;
    drag = null;
    if (swipe.axis === null || !figure) return;
    swipedAt = performance.now();
    const speed = swipeSpeed(swipe.moves);
    if (swipe.axis === 'y') {
      if (!mobile) return;
      if (dismissOffset.current > 0) releaseDismiss(speed);
      else releaseInfo(swipe.startMode, speed);
      return;
    }
    const width = figure.clientWidth;
    let delta = 0;
    if (swipe.dx < -width * SWIPE_SHARE || speed < -FLICK_SPEED) delta = 1;
    else if (swipe.dx > width * SWIPE_SHARE || speed > FLICK_SPEED) delta = -1;
    slide(delta);
  }

  function onkeydown(e: KeyboardEvent) {
    if (viewer.current === null || !isShortcut(e)) return;
    if (e.key === 'Escape' && !tagMenu.current) viewer.close();
    else if (e.key === 'ArrowLeft') slide(-1);
    else if (e.key === 'ArrowRight') slide(1);
    else if (e.key === 'f') library.toggleLike(viewer.current);
    else if (e.key === 'd') library.toggleDislike(viewer.current);
  }
</script>

<svelte:window {onkeydown} />

{#snippet slidesAndArrows()}
  {#each slides as slide (slide.id)}
    {@const shown = details.get(slide.id)}
    {@const media = shown ?? library.post(slide.id)}
    {@const failure = failures.get(slide.id)}
    <div
      class="absolute inset-0 flex items-center justify-center [container-type:size]"
      style:transform="translateX(calc({slide.delta} * (100% + {SLIDE_GAP}px) + {slideOffset.current}px)) scaleX({slide.delta === 0 ? 1 + Math.abs(stretch.current) : 1})"
      style:transform-origin={stretch.current > 0 ? 'left' : 'right'}
      data-delta={slide.delta}
    >
      {#if shown?.blockedBy.length}
        <p class="p-4 text-center opacity-70">It has a blacklisted tag: {shown.blockedBy.join(', ')}.</p>
      {:else if media}
        <PostMedia post={media} active={slide.delta === 0} controls site={settings.site} />
      {:else if failure !== undefined}
        <ErrorNote error={failure} />
      {:else}
        <span class="loading loading-spinner"></span>
      {/if}
      {#if likeBurst?.post === slide.id}<LikeBurst count={likeBurst.count} />{/if}
    </div>
  {/each}
  {#if barsShown}
    {#each [['M15 18l-6-6 6-6', -1, 'left-2'], ['M9 18l6-6-6-6', 1, 'right-2']] as const as [chevron, delta, side] (delta)}
      <button
        class={['btn btn-circle absolute top-1/2 z-[1] -translate-y-1/2 border-white/20 bg-black/45 text-white hover:bg-black/70', side]}
        aria-label={delta < 0 ? 'Previous post' : 'Next post'}
        onclick={(e) => {
          e.stopPropagation();
          slide(delta);
        }}
      >
        <svg class="size-6" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"
          ><path d={chevron} /></svg
        >
      </button>
    {/each}
  {/if}
{/snippet}

{#if viewer.shown}
  <div
    class={[
      'fixed inset-0 z-(--z-viewer) text-base-content',
      mobile ? 'overflow-hidden' : 'grid grid-rows-[minmax(0,1fr)_auto] split:grid-cols-[280px_minmax(0,1fr)]',
    ]}
    style:background-color={`rgb(0 0 0 / ${BACKDROP_OPACITY * (mobile ? 1 - Math.min(1, dismissOffset.current / Math.max(1, viewerHeight)) : 1)})`}
    data-theme="dracula"
    role="dialog"
    aria-modal="true"
    aria-label="Post viewer"
    tabindex="-1"
    bind:clientHeight={viewerHeight}
  >
    {#if !mobile && tagsPost && !tagsPost.blockedBy.length}
      <aside
        class={[
          'overflow-y-auto overscroll-contain border-r border-base-300 bg-base-200 px-4 pt-3 pb-6 no-scrollbar split:row-span-2',
          'max-split:absolute max-split:inset-y-0 max-split:left-0 max-split:z-10 max-split:w-[min(85vw,320px)] max-split:shadow-xl',
          !tagsOpen && 'max-split:hidden',
        ]}
      >
        <button class="btn btn-circle btn-sm sticky top-0 float-right split:hidden" aria-label="Close tags" onclick={() => (tagsOpen = false)}>✕</button>
        {#if viewer.current !== null && failures.has(viewer.current)}<ErrorNote error={failures.get(viewer.current)} />{/if}
        <div class={['transition-opacity', tagsPost !== post && 'opacity-50']}>
          <PostTags post={tagsPost} target={staged} site={settings.site} />
        </div>
      </aside>
    {/if}

    <figure
      bind:this={figure}
      class={[
        'm-0 min-h-0 min-w-0 touch-none overflow-hidden select-none',
        mobile ? 'absolute inset-x-0 top-0' : 'relative split:col-start-2',
      ]}
      style:height={mobile ? `${(1 - progress * SHEET_SHARE) * 100}%` : undefined}
      style:transform={mobile && dismissOffset.current ? `translateY(${dismissOffset.current}px)` : undefined}
      onclick={onMediaClick}
      role="presentation"
      {onpointerdown}
      {onpointermove}
      {onpointerup}
      onpointercancel={onpointerup}
      ondragstart={(e) => e.preventDefault()}
      {@attach taps({ double: likeFromTap, single: mobile ? toggleBars : undefined })}
    >
      {@render slidesAndArrows()}
    </figure>

    {#if mobile}
      <div
        class={[
          'pointer-events-none absolute inset-x-0 top-0 z-[2] flex items-center justify-between bg-gradient-to-b from-black/70 to-transparent p-2 transition-opacity',
          barsShown ? 'opacity-100 *:pointer-events-auto' : 'opacity-0',
        ]}
      >
        <button class="btn btn-circle btn-ghost text-2xl text-white" aria-label="Back" onclick={() => viewer.close()}>‹</button>
        <button class={['btn btn-circle text-lg', mode === 'info' ? 'btn-primary' : 'btn-ghost text-white']} aria-label="Info" onclick={toggleInfo}>ℹ️</button>
      </div>
      <nav
        class={[
          'absolute inset-x-0 bottom-0 z-[2] flex flex-wrap items-center justify-center gap-1.5 bg-gradient-to-t from-black/70 to-transparent p-2 pt-6 transition-opacity',
          barsShown && progress === 0 ? 'opacity-100' : 'pointer-events-none opacity-0',
        ]}
      >
        {#if viewer.turnError}<div class="w-full"><ErrorNote error={viewer.turnError} /></div>{/if}
        {#if post && !post.blockedBy.length}
          <ViewerActions {post} site={settings.site} />
        {/if}
        
      </nav>
      <div
        class="absolute inset-x-0 bottom-0 z-[3]"
        style:height="{SHEET_SHARE * 100}%"
        style:transform="translateY({(1 - progress) * 100}%)"
      >
        {#if progress > 0}
          <InfoSheet {post} target={staged} site={settings.site} turnError={viewer.turnError} ondrag={(dy) => dragInfo(1, dy)} onrelease={(speed) => releaseInfo('info', speed)} />
        {/if}
      </div>
    {:else}
      <div
        class="flex flex-wrap items-center justify-center gap-1.5 bg-black/50 p-2 split:col-start-2"
        role="presentation"
        onclick={(e) => e.target === e.currentTarget && viewer.close()}
      >
        {#if viewer.turnError}<div class="w-full"><ErrorNote error={viewer.turnError} /></div>{/if}
        {#if post && !post.blockedBy.length}
          <button class={[BAR_BUTTON, 'split:hidden']} onclick={() => (tagsOpen = !tagsOpen)}>Tags</button>
          <ViewerActions {post} site={settings.site} />
        {/if}
        <button class={BAR_BUTTON} aria-label="Close" title="Close (Esc)" onclick={() => viewer.close()}>✕</button>
        
      </div>
    {/if}
  </div>
{/if}
