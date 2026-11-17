<script lang="ts">
  import type { ClassValue } from 'svelte/elements';

  let { text, class: className }: { text: string; class?: ClassValue } = $props();

  let element = $state<HTMLElement>();
  let expanded = $state(false);
  let overflowing = $state(false);

  $effect(() => {
    if (!element) return;
    const measured = element;
    const measure = () => (overflowing = measured.scrollWidth > measured.clientWidth);
    const observer = new ResizeObserver(measure);
    observer.observe(measured);
    measure();
    return () => observer.disconnect();
  });
</script>

<span class="flex min-w-0 items-start gap-1">
  <span bind:this={element} class={['min-w-0', expanded ? 'break-all' : 'truncate', className]}>{text}</span>
  {#if overflowing || expanded}
    <button
      type="button"
      class="btn btn-ghost btn-xs h-5 min-h-0 shrink-0 px-1"
      title={expanded ? 'Show less' : 'Show all'}
      aria-expanded={expanded}
      onclick={() => (expanded = !expanded)}>{expanded ? 'less' : '…'}</button
    >
  {/if}
</span>
