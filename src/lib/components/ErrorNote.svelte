<script lang="ts">
  import { resolve } from '$app/paths';
  import { errorMessage, isAppError } from '$lib/api';
  import PageMessage from './PageMessage.svelte';

  let { error, centered = false }: { error: unknown; centered?: boolean } = $props();
</script>

{#snippet note()}
  <div class={['flex flex-wrap items-center gap-2 text-sm opacity-80', centered ? 'justify-center' : 'my-2']}>
    <span>{errorMessage(error)}</span>
    {#if isAppError(error) && (error.kind === 'missingApiKey' || error.kind === 'missingProxy')}
      <a class="btn btn-primary btn-xs" href={resolve('/settings')}>Open Settings</a>
    {/if}
  </div>
{/snippet}

{#if centered}
  <PageMessage>{@render note()}</PageMessage>
{:else}
  {@render note()}
{/if}
