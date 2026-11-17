<script lang="ts">
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
</script>

<div class="pointer-events-none absolute inset-0 grid place-items-center p-6">
  <div class="pointer-events-auto max-w-sm text-center text-balance">{@render children()}</div>
</div>
