<script module lang="ts">
  const fileSizes = new Map<number, Promise<number | null>>();

  function sizeOf(id: number) {
    let size = fileSizes.get(id);
    if (!size) {
      size = api.fileSize(id).catch((error) => {
        fileSizes.delete(id);
        console.error('Reading the file size failed', error);
        return null;
      });
      fileSizes.set(id, size);
    }
    return size;
  }
</script>

<script lang="ts">
  import { api, type PostDetail } from '$lib/api';
  import { formatBytes } from '$lib/format';
  import { signal } from '$lib/signals';
  import { library } from '$lib/state/library.svelte';
  import type { TagTarget } from '$lib/state/search.svelte';
  import { tagMenu } from '$lib/state/tagMenu.svelte';
  import { tagText } from '$lib/tags';
  import PostFamily from './PostFamily.svelte';
  import SectionTitle from './SectionTitle.svelte';

  let { post, target, onrelated }: { post: PostDetail; target: TagTarget; onrelated?: (id: number, ids: number[]) => void } = $props();

  let fileSize = $state<number | null>(null);

  $effect(() => {
    const id = post.id;
    fileSize = null;
    sizeOf(id).then((size) => {
      if (post.id === id) fileSize = size;
    });
  });
</script>

<div class="text-sm">
  <PostFamily id={post.id} onopen={onrelated} />
  {#each post.tagGroups as group (group.label)}
    <SectionTitle>{group.label}</SectionTitle>
    <ul>
      {#each group.tags as tag (tag.name)}
        {@const state = target.state(tag.name)}
        {@const blacklisted = library.blacklisted.has(tag.name)}
        <li class="flex items-baseline justify-between gap-2">
          <button
            type="button"
            class={[
              'cursor-pointer py-0.5 text-left break-all hover:underline',
              state === 'exclude' ? 'text-error line-through' : tagText(tag.type),
              state === 'include' && 'font-semibold',
              blacklisted && 'line-through opacity-60',
            ]}
            onclick={(e) => {
              signal(post.id, 'tag', tag.name);
              tagMenu.show(e.currentTarget, tag.name, tag.type, target);
            }}
          >
            {state === 'include' ? '+ ' : ''}{tag.name}{blacklisted ? ' (blacklisted)' : ''}
          </button>
          {#if tag.count !== null}<small class="text-xs opacity-50">{tag.count.toLocaleString()}</small>{/if}
        </li>
      {/each}
    </ul>
  {/each}
  <SectionTitle>Info</SectionTitle>
  <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5">
    <dt class="opacity-60">Score</dt>
    <dd>{post.score}</dd>
    {#if post.width}
      <dt class="opacity-60">Dimensions</dt>
      <dd>{post.width} × {post.height}</dd>
    {/if}
    {#if post.postedAt !== null}
      <dt class="opacity-60">Posted</dt>
      <dd>{new Date(post.postedAt).toISOString().slice(0, 10)}</dd>
    {/if}
    {#if fileSize !== null}
      <dt class="opacity-60">File size</dt>
      <dd>{formatBytes(fileSize)}</dd>
    {/if}
    <dt class="opacity-60">Type</dt>
    <dd>{post.fileExt}</dd>
    <dt class="opacity-60">Age rating</dt>
    <dd class="capitalize">{post.rating}</dd>
  </dl>
</div>
