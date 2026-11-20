<script lang="ts">
  import SectionTitle from './SectionTitle.svelte';
  import type { Snippet } from 'svelte';

  let { title, trailing, children }: { title: string; trailing?: Snippet; children: Snippet } = $props();
</script>

<section class="mb-5 break-inside-avoid">
  <SectionTitle {trailing}>{title}</SectionTitle>
  <ul class="divide-y divide-base-300 rounded-box border border-base-300 bg-base-100">
    {@render children()}
  </ul>
</section>
