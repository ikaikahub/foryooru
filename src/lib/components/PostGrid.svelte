<script lang="ts">
  import { imageUrl, showsOriginal, videoUrl, type Post } from '$lib/api';
  import { postUrl } from '$lib/sites';
  import { image, playWhile, PRELOAD_AHEAD, seen, whileVisible } from '$lib/attachments';
  import { postMenu } from '$lib/state/postMenu.svelte';
  import { MAX_COLUMNS, settings } from '$lib/state/settings.svelte';
  import { viewer, type PageTurner } from '$lib/state/viewer.svelte';
  import type { Attachment } from 'svelte/attachments';
  import { SvelteSet } from 'svelte/reactivity';
  import MediaLabel from './MediaLabel.svelte';
  import ReactionButtons from './ReactionButtons.svelte';

  let {
    posts,
    pageTurner = null,
    trackSeen = true,
  }: { posts: Post[]; pageTurner?: PageTurner | null; trackSeen?: boolean } = $props();

  const AUTO_COLUMN_WIDTH = 400;
  const GAP = 4;

  let width = $state(0);
  const onScreen = new SvelteSet<number>();
  const reached = new SvelteSet<number>();
  const animated = new SvelteSet<number>();

  const near = $derived.by(() => {
    const indexes = posts.flatMap((post, index) => (onScreen.has(post.id) ? [index] : []));
    if (!indexes.length) return new Set<number>();
    const last = indexes[indexes.length - 1] + PRELOAD_AHEAD;
    return new Set(posts.slice(indexes[0], last + 1).map((post) => post.id));
  });

  $effect(() => {
    for (const id of near) reached.add(id);
  });

  const watchScreen =
    (id: number): Attachment =>
    (element) => {
      const stop = whileVisible((visible) => (visible ? onScreen.add(id) : onScreen.delete(id)))(element);
      return () => {
        stop?.();
        onScreen.delete(id);
      };
    };

  function noteAnimated(img: EventTarget, id: number) {
    if (showsOriginal(img, id)) animated.add(id);
  }

  const autoplays = (post: Post) => post.media !== 'image' && settings.gridAutoplay;
  const animating = (post: Post) => autoplays(post) && near.has(post.id);
  const countsAsSeen = (post: Post) => trackSeen && (post.media === 'image' || animated.has(post.id));

  const columnCount = $derived(settings.columns || Math.max(1, Math.min(MAX_COLUMNS, Math.ceil(width / AUTO_COLUMN_WIDTH))));

  const columns = $derived.by(() => {
    const columnWidth = Math.max(1, (width - GAP * (columnCount - 1)) / columnCount);
    const filled = Array.from({ length: columnCount }, () => ({ posts: [] as Post[], height: 0 }));
    for (const post of posts) {
      const shortest = filled.reduce((a, b) => (b.height < a.height ? b : a));
      shortest.posts.push(post);
      shortest.height += (columnWidth * post.previewHeight) / Math.max(1, post.previewWidth) + GAP;
    }
    return filled;
  });

  const MIDDLE_BUTTON = 1;

  function open(e: MouseEvent, post: Post) {
    const url = postUrl(post);
    if ((e.ctrlKey || e.metaKey) && url) window.open(url, '_blank', 'noopener');
    else viewer.open(post.id, posts.map((p) => p.id), pageTurner);
  }

  function openElsewhere(e: MouseEvent, post: Post) {
    const url = postUrl(post);
    if (e.button === MIDDLE_BUTTON && url) window.open(url, '_blank', 'noopener');
  }
</script>

<div class="flex items-start" style:gap="{GAP}px" bind:clientWidth={width}>
  {#each columns as column, index (index)}
    <div class="flex min-w-0 flex-1 flex-col" style:gap="{GAP}px">
      {#each column.posts as post (post.id)}
        <article
          class="group relative overflow-hidden border border-base-300 bg-base-200"
          {@attach watchScreen(post.id)}
          {@attach countsAsSeen(post) && seen(post.id)}
        >
          <button
            type="button"
            class="relative block w-full cursor-pointer bg-base-300"
            style:aspect-ratio="{post.previewWidth || 1} / {post.previewHeight || 1}"
            onclick={(e) => open(e, post)}
            onauxclick={(e) => openElsewhere(e, post)}
            oncontextmenu={(e) => postMenu.open(e, post)}
          >
            {#if post.media === 'video' && animating(post)}
              <video
                class="block size-full object-cover"
                src={videoUrl(post.id)}
                poster={imageUrl('sample', post.id)}
                onplaying={() => animated.add(post.id)}
                {@attach playWhile(onScreen.has(post.id))}
                loop
                muted
                playsinline
                preload="auto"
              ></video>
            {:else}
              <img
                class="block size-full"
                alt=""
                onload={(e) => noteAnimated(e.currentTarget, post.id)}
                {@attach image(imageUrl(animating(post) ? 'original' : 'sample', post.id), reached.has(post.id))}
              />
            {/if}
            {#if post.media !== 'image' && !(animating(post) && animated.has(post.id))}
              <MediaLabel ext={post.fileExt} class="top-1 right-1" />
            {/if}
          </button>
          <div class="absolute bottom-0 left-0 flex gap-0">
            <ReactionButtons id={post.id} class="btn-sm btn-square" bare />
          </div>
        </article>
      {/each}
    </div>
  {/each}
</div>
