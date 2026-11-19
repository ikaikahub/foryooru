<script lang="ts">
  import type { PostDetail, Site } from '$lib/api';
  import { touchDrag } from '$lib/attachments';
  import { swipeAxis, swipeSpeed, trackMove, type Moves } from '$lib/motion';
  import type { TagTarget } from '$lib/state/search.svelte';
  import { settings } from '$lib/state/settings.svelte';
  import ApiNote from './ApiNote.svelte';
  import ErrorNote from './ErrorNote.svelte';
  import PostTags from './PostTags.svelte';
  import ViewerActions from './ViewerActions.svelte';

  let {
    post,
    site = settings.site,
    target,
    actions = true,
    turnError = null,
    ondrag,
    onrelease,
    onrelated,
  }: {
    post: PostDetail | null;
    site?: Site;
    target: TagTarget;
    actions?: boolean;
    turnError?: unknown;
    ondrag: (dy: number) => void;
    onrelease: (speed: number) => void;
    onrelated?: (id: number, ids: number[]) => void;
  } = $props();

  const foreign = $derived(site !== settings.site);

  let list = $state<HTMLElement>();
  let drag: { x0: number; y0: number; mode: 'drag' | 'native' | null; moves: Moves } | null = null;

  function ontouchstart(e: TouchEvent) {
    drag = e.touches.length === 1 ? { x0: e.touches[0].clientX, y0: e.touches[0].clientY, mode: null, moves: [] } : null;
  }

  function ontouchmove(e: TouchEvent) {
    if (!drag) return;
    const touch = e.touches[0];
    const dy = touch.clientY - drag.y0;
    if (drag.mode === null) {
      const axis = swipeAxis(touch.clientX - drag.x0, dy);
      if (!axis) return;
      drag.mode = axis === 'y' && dy > 0 && (list?.scrollTop ?? 0) <= 0 ? 'drag' : 'native';
    }
    if (drag.mode !== 'drag') return;
    e.preventDefault();
    ondrag(Math.max(0, dy));
    drag.moves = trackMove(drag.moves, e.timeStamp, touch.clientY);
  }

  function ontouchend() {
    const finished = drag;
    drag = null;
    if (finished?.mode === 'drag') onrelease(swipeSpeed(finished.moves));
  }
</script>

<div class="flex h-full flex-col rounded-t-2xl bg-base-200 text-sm" {@attach touchDrag({ start: ontouchstart, move: ontouchmove, end: ontouchend })}>
  <div class="flex shrink-0 justify-center pt-2 pb-1"><span class="block h-1.5 w-10 rounded-full bg-base-content/40"></span></div>
  <div class="min-h-0 flex-1 touch-pan-y overflow-y-auto overscroll-contain px-3 pb-6" bind:this={list}>
    {#if turnError}<ErrorNote error={turnError} />{/if}
    {#if post && !post.blockedBy.length}
      {#if actions}<div class="flex flex-wrap justify-center gap-1.5 py-2"><ViewerActions {post} {site} /></div>{/if}
      {#if foreign}<ApiNote {site} />{/if}
      <PostTags {post} {target} {onrelated} {site} />
    {/if}
  </div>
</div>
