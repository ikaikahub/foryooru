<script lang="ts">
  import { pageScroller } from '$lib/pageScroller.svelte';
  import type { Snippet } from 'svelte';
  import type { ClassValue } from 'svelte/elements';

  let { class: className, children }: { class?: ClassValue; children: Snippet } = $props();
</script>

<div class="min-h-0 flex-1 overflow-auto" {@attach pageScroller}>
  <table class={['table table-sm table-pin-rows', className]}>
    {@render children()}
  </table>
</div>
