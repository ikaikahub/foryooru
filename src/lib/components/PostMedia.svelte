<script lang="ts">
  import { imageUrl, showsOriginal, videoUrl, type Post } from '$lib/api';
  import { image, playWhile } from '$lib/attachments';
  import { signal } from '$lib/signals';
  import { postMenu } from '$lib/state/postMenu.svelte';
  import { zoomable } from '$lib/zoom';
  import type { Snippet } from 'svelte';

  let {
    post,
    active,
    preload = false,
    controls = false,
    img = $bindable(),
    onloaded,
    children,
  }: {
    post: Post;
    active: boolean;
    preload?: boolean;
    controls?: boolean;
    img?: HTMLImageElement;
    onloaded?: () => void;
    children?: Snippet;
  } = $props();

  const LOOP_GAP_S = 1;

  let started = $state(false);
  let painted = $state(false);
  let playedUntil = 0;
  let looped = false;

  function noticeLoop(video: HTMLVideoElement) {
    if (active && !looped && video.currentTime < playedUntil - LOOP_GAP_S) {
      looped = true;
      signal(post.id, 'videoLooped');
    }
    playedUntil = video.currentTime;
  }

  const sample = $derived(imageUrl('sample', post.id));
  const original = $derived(imageUrl('original', post.id));
  const ratio = $derived((post.width || post.previewWidth || 1) / (post.height || post.previewHeight || 1));
  const width = $derived(post.width ? `min(100cqw, 100cqh * ${ratio}, ${post.width}px)` : `min(100cqw, 100cqh * ${ratio})`);

  $effect(() => {
    if (active || preload) started = true;
  });

  $effect(() => {
    void post.id;
    painted = false;
  });

  const source = $derived(started ? videoUrl(post.id) : undefined);
</script>

<div
  class="relative bg-cover bg-center"
  style:width
  style:aspect-ratio={ratio}
  style:background-image={painted ? undefined : `url(${imageUrl('thumbnail', post.id)})`}
>
  {#if post.media === 'video'}
    <video
      class="block size-full object-contain"
      src={source}
      poster={sample}
      controls={controls && active}
      oncontextmenu={(e) => postMenu.open(e, post)}
      onplaying={onloaded}
      onloadeddata={() => (painted = true)}
      ontimeupdate={(e) => noticeLoop(e.currentTarget)}
      {@attach playWhile(active && source !== undefined)}
      loop
      muted
      playsinline
      preload={active || preload ? 'auto' : 'none'}
    ></video>
  {:else}
    <img
      bind:this={img}
      class="block size-full object-contain"
      alt=""
      oncontextmenu={(e) => postMenu.open(e, post)}
      onload={(e) => {
        painted = true;
        if (post.media === 'gif' && showsOriginal(e.currentTarget, post.id)) onloaded?.();
      }}
      {@attach image(post.media === 'gif' && (active || preload) ? original : sample)}
      {@attach zoomable(original)}
    />
  {/if}
  {@render children?.()}
</div>
