<script module lang="ts">
  export const PULL_THRESHOLD = 72;
  const MAX_PULL = 120;
  const RESISTANCE = 0.5;

  export const pullDistance = (dy: number) => Math.min(MAX_PULL, Math.max(0, dy) * RESISTANCE);
</script>

<script lang="ts">
  let { distance }: { distance: number } = $props();

  const ready = $derived(distance >= PULL_THRESHOLD);
</script>

{#if distance > 0}
  <div
    class="pointer-events-none fixed top-(--top-bar) left-1/2 z-(--z-feed-overlay) grid size-10 place-items-center rounded-full bg-base-200 text-lg text-base-content shadow-lg"
    style:transform="translate(-50%, {distance - 40}px) rotate({distance * 3}deg)"
    style:opacity={Math.min(1, distance / PULL_THRESHOLD)}
    aria-hidden="true"
  >
    <span class={ready ? 'text-primary' : 'opacity-70'}>↻</span>
  </div>
{/if}
