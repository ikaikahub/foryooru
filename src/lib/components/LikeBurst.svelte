<script lang="ts">
  let { count }: { count: number } = $props();
</script>

{#key count}
  {#if count}
    <span class="like-burst pointer-events-none absolute top-1/2 left-1/2 z-[3] text-8xl drop-shadow-lg" aria-hidden="true">❤️</span>
  {/if}
{/key}
