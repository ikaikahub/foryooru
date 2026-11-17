<script lang="ts">
  import { fade } from 'svelte/transition';

  let { text }: { text: string } = $props();
</script>

<div
  transition:fade|global={{ duration: 150 }}
  class="pointer-events-none fixed inset-x-0 top-(--top-bar) z-(--z-loading) flex justify-center pt-2"
  role="status"
  aria-live="polite"
>
  <div class="absolute inset-x-0 top-0 h-0.75 bg-primary"></div>
  <span class="flex items-center gap-2 rounded-full border border-base-300 bg-base-200 px-3 py-1 text-sm shadow">
    <span class="loading loading-spinner loading-xs"></span>{text}
  </span>
</div>
