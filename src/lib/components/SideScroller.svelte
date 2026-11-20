<script lang="ts">
  import type { Snippet } from 'svelte';

  // each arrow click scrolls this share of the visible width, so a little of the last view stays for context
  const PAGE_SHARE = 0.8;
  const EDGE_PX = 2;
  const ARROW =
    'btn btn-circle btn-sm absolute top-1/2 z-[1] -translate-y-1/2 border-base-300 bg-base-100/85 opacity-0 shadow transition-opacity group-hover/scroller:opacity-100 [@media(hover:none)]:hidden';

  let {
    children,
    class: className = '',
    element = $bindable(),
    onscroll,
  }: {
    children: Snippet;
    class?: string;
    element?: HTMLElement;
    onscroll?: (e: Event & { currentTarget: HTMLElement }) => void;
  } = $props();

  let atStart = $state(true);
  let atEnd = $state(true);

  // the arrows only show on hover, so checking on hover and on scroll keeps them right as images load in
  function updateEnds() {
    if (!element) return;
    atStart = element.scrollLeft <= EDGE_PX;
    atEnd = element.scrollLeft + element.clientWidth >= element.scrollWidth - EDGE_PX;
  }

  function page(direction: -1 | 1) {
    element?.scrollBy({ left: direction * element.clientWidth * PAGE_SHARE, behavior: 'smooth' });
  }
</script>

<!-- drawn rather than typed, a text arrow sits below the middle of its line -->
{#snippet chevron(path: string)}
  <svg class="size-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d={path} />
  </svg>
{/snippet}

<div class="group/scroller relative" role="presentation" onpointerenter={updateEnds}>
  <div
    bind:this={element}
    class={['no-scrollbar overflow-x-auto', className]}
    onscroll={(e) => {
      updateEnds();
      onscroll?.(e);
    }}
  >
    {@render children()}
  </div>
  {#if !atStart}
    <button type="button" class={[ARROW, 'left-1']} aria-label="Scroll left" onclick={() => page(-1)}>{@render chevron('M15 6l-6 6 6 6')}</button>
  {/if}
  {#if !atEnd}
    <button type="button" class={[ARROW, 'right-1']} aria-label="Scroll right" onclick={() => page(1)}>{@render chevron('M9 6l6 6-6 6')}</button>
  {/if}
</div>
