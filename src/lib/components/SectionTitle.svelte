<script lang="ts">
  import type { Snippet } from 'svelte';

  let { children, trailing }: { children: Snippet; trailing?: Snippet } = $props();
</script>

<h3
  class="mt-4 mb-1 flex items-center justify-between gap-3 text-xs font-semibold tracking-wide uppercase opacity-60 first:mt-0"
>
  <span>{@render children()}</span>
  {#if trailing}<span class="shrink-0 truncate normal-case">{@render trailing()}</span>{/if}
</h3>
