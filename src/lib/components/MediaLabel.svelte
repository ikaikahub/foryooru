<script lang="ts">
  import type { ClassValue } from 'svelte/elements';

  let { ext, class: className }: { ext: string; class?: ClassValue } = $props();
</script>

<span class={['absolute rounded-md bg-black/65 px-1.5 py-0.5 text-xs font-bold tracking-wide text-white', className]}>{ext}</span>
