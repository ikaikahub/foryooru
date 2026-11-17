<script lang="ts">
  import { toast } from '$lib/state/toast.svelte';
  import { fly } from 'svelte/transition';
</script>

{#if toast.message}
  <div class="toast toast-center toast-bottom z-(--z-toast) mb-(--bottom-bar)" role="status">
    {#key toast.message}
      <div class="alert max-w-[90vw] text-sm break-all whitespace-normal shadow-lg" in:fly={{ y: 12, duration: 150 }}>{toast.message}</div>
    {/key}
  </div>
{/if}
