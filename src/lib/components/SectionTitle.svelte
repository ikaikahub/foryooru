<script lang="ts">
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
</script>

<h3 class="mt-4 mb-1 text-xs font-semibold tracking-wide uppercase opacity-60 first:mt-0">{@render children()}</h3>
