<script lang="ts">
  import { goto } from '$app/navigation';
  import { isShortcut } from '$lib/keys';
  import { viewer } from '$lib/state/viewer.svelte';

  let { current, pages, href }: { current: number; pages: number; href: (page: number) => string } = $props();

  const shown = $derived.by(() => {
    const numbers = [1, pages, current - 2, current - 1, current, current + 1, current + 2];
    return [...new Set(numbers)].filter((n) => n >= 1 && n <= pages).sort((a, b) => a - b);
  });

  function onkeydown(e: KeyboardEvent) {
    if (viewer.current !== null || !isShortcut(e)) return;
    const next = { ArrowLeft: current - 1, ArrowRight: current + 1 }[e.key];
    if (next && next >= 1 && next <= pages) {
      e.preventDefault();
      goto(href(next));
    }
  }
</script>

<svelte:window {onkeydown} />

{#if pages > 1}
  <nav class="mt-2 mb-2 flex flex-wrap justify-center gap-1">
    {#if current > 1}<a class="btn btn-sm" href={href(current - 1)}>‹ Prev</a>{/if}
    {#each shown as number, index (number)}
      {#if index > 0 && number - shown[index - 1] > 1}<span class="px-1 self-center">…</span>{/if}
      <a class={['btn btn-sm min-w-9', number === current && 'btn-primary']} href={href(number)} aria-current={number === current ? 'page' : undefined}>
        {number}
      </a>
    {/each}
    {#if current < pages}<a class="btn btn-sm" href={href(current + 1)}>Next ›</a>{/if}
  </nav>
{/if}
