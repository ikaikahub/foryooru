<script lang="ts">
  import type { Site } from '$lib/api';
  import { SITES } from '$lib/sites';

  let { site }: { site: Site } = $props();
</script>

<p class="w-full px-2 py-0.5 text-center text-xs opacity-60">This post is from the {SITES[site].label} API</p>
