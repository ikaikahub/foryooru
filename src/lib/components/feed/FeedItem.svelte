<script lang="ts">
  import type { PostDetail } from '$lib/api';
  import { searchSimilar } from '$lib/postActions';
  import { taps } from '$lib/attachments';
  import { library } from '$lib/state/library.svelte';
  import type { TagTarget } from '$lib/state/search.svelte';
  import { resetZoom } from '$lib/zoom';
  import type { Attachment } from 'svelte/attachments';
  import LikeBurst from '../LikeBurst.svelte';
  import MediaLabel from '../MediaLabel.svelte';
  import PopoverMenu from '../PopoverMenu.svelte';
  import ArtistBar from '../ArtistBar.svelte';
  import PostMedia from '../PostMedia.svelte';
  import ReactionButtons from '../ReactionButtons.svelte';
  import ShareItems from '../ShareItems.svelte';

  let {
    post,
    current,
    preload,
    loaded,
    watch,
    onloaded,
    ontags,
    onsearch,
    target,
  }: {
    post: PostDetail;
    target: TagTarget;
    current: boolean;
    preload: boolean;
    loaded: boolean;
    watch: Attachment;
    onloaded: () => void;
    ontags?: () => void;
    onsearch?: () => void;
  } = $props();

  const SIDE_BUTTON = 'btn btn-circle size-12 border-white/25 bg-black/60 text-xl text-white';
  const SHARE_ITEM = 'justify-end text-right';

  let img = $state<HTMLImageElement>();
  let shareAnchor = $state<HTMLElement | null>(null);
  let likeBursts = $state(0);

  function likeFromTap() {
    library.like(post.id);
    likeBursts++;
  }

  $effect(() => {
    if (!current) resetZoom(img);
  });
</script>

<section
  class="relative flex h-full snap-start snap-always items-center justify-center overflow-hidden select-none"
  {@attach watch}
  {@attach taps({ double: likeFromTap })}
>
  <LikeBurst count={likeBursts} />
  <div class="flex size-full items-center justify-center [container-type:size]">
    <PostMedia {post} active={current} {preload} controls bind:img {onloaded}>
      {#if !loaded}
        <MediaLabel ext={post.fileExt} class="top-2 right-2" />
        <div class="pointer-events-none absolute inset-0 grid place-items-center">
          <span class="loading loading-spinner loading-lg"></span>
        </div>
      {/if}
    </PostMedia>
  </div>

  <!-- adding the exact same right-2 bottom0.5 rem as in ./+layout:54-->
  <!-- adding 0.5 rem to calc so it looks evenly cornered -->
  <ArtistBar {post} {target} class="absolute bottom-3 left-3 z-[4] max-w-[calc(100%-5rem)]" />
  <div class="absolute right-2 bottom-2 z-[4] flex flex-col gap-3">
    {#if onsearch}<button class={SIDE_BUTTON} title="Search" onclick={onsearch}>🔍</button>{/if}
    <ReactionButtons id={post.id} class={SIDE_BUTTON} />
    {#if ontags}<button class={SIDE_BUTTON} title="Tags and info (i)" onclick={ontags}>ℹ️</button>{/if}
    <button class={SIDE_BUTTON} title="Search for posts like this one" onclick={() => searchSimilar(post)}>🪄</button>
    <button
      class={SIDE_BUTTON}
      title="Share or save"
      onclick={(e) => (shareAnchor = shareAnchor ? null : e.currentTarget)}>📤</button
    >
  </div>
  <PopoverMenu anchor={shareAnchor} placement="left" class="menu-md w-max" onclose={() => (shareAnchor = null)}>
    <ShareItems {post} class={SHARE_ITEM} iconEnd />
  </PopoverMenu>
</section>
