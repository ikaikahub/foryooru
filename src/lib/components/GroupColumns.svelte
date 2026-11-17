<script lang="ts">
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
</script>

<div class="@container">
  <div class="columns-1 gap-6 @3xl:columns-2">
    {@render children()}
  </div>
</div>
