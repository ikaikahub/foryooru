<script module lang="ts">
  type Strip = { family: PostFamily; scrollLeft: number };

  const strips = new Map<number, Strip>();
  let lastShown: Strip | null = null;

  function remember(family: PostFamily) {
    const strip = { family, scrollLeft: 0 };
    if (family.relation === 'family') for (const post of family.posts) strips.set(post.id, strip);
    return strip;
  }
</script>

<script lang="ts">
  import { page } from '$app/state';
  import { api, imageUrl, type PostFamily } from '$lib/api';
  import { splitQuery, urlQuery } from '$lib/state/search.svelte';
  import { viewer } from '$lib/state/viewer.svelte';
  import { untrack } from 'svelte';
  import SectionTitle from './SectionTitle.svelte';

  let { id, onopen = (related, ids) => viewer.show(related, ids) }: { id: number; onopen?: (id: number, ids: number[]) => void } = $props();

  let strip = $state.raw<Strip | null>(null);
  let row = $state<HTMLElement>();
  const family = $derived(strip?.family ?? null);
  const ids = $derived(family?.posts.map((post) => post.id) ?? []);
  const hasOthers = $derived(ids.some((other) => other !== id));

  $effect(() => {
    const current = id;
    const shown = untrack(() => strip) ?? lastShown;
    if (shown?.family.posts.some((post) => post.id === current)) {
      strip = shown;
      return;
    }
    const known = strips.get(current);
    if (known) {
      strip = known;
      return;
    }
    strip = null;
    api
      .postFamily(current, splitQuery(urlQuery(page.url)))
      .then((found) => {
        if (id === current) strip = remember(found);
      })
      .catch((error) => console.error('Finding related posts failed', error));
  });

  $effect(() => {
    if (strip) lastShown = strip;
  });

  $effect(() => {
    if (row && strip) row.scrollLeft = strip.scrollLeft;
  });
</script>

{#if !family || hasOthers}
  <SectionTitle>Related posts</SectionTitle>
{/if}
{#if !family}
  <div class="flex h-[100px] items-center justify-center mb-2">
    <span class="loading loading-spinner loading-md opacity-60"></span>
  </div>
{:else if hasOthers}
  <div
    bind:this={row}
    class="-mx-1 flex gap-2 overflow-x-auto px-1 pb-2"
    onscroll={(e) => {
      if (strip) strip.scrollLeft = e.currentTarget.scrollLeft;
    }}
  >
    {#each family.posts as related, index (related.id)}
      <button
        type="button"
        class={[
          'shrink-0 overflow-hidden rounded-md border-2',
          related.id === id ? 'border-primary' : family.relation === 'family' && index === 0 ? 'border-success' : 'border-transparent',
        ]}
        title={family.relation === 'family' && index === 0 ? 'Parent' : undefined}
        onclick={() => related.id !== id && onopen(related.id, ids)}
      >
        <img class="block h-24 w-auto max-w-none" src={imageUrl('thumbnail', related.id)} alt="" loading="lazy" />
      </button>
    {/each}
  </div>
{/if}
