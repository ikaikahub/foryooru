<script lang="ts">
  import type { FetchStats, LeadTag } from '$lib/api';
  import SettingsGroup from '../SettingsGroup.svelte';
  import { NOTE, percent } from './stats';

  let { stats }: { stats: FetchStats } = $props();
</script>

{#snippet leads(title: string, tags: LeadTag[])}
  {#if tags.length}
    <li class={NOTE}>
      <div class="mb-1 text-xs font-semibold opacity-70">{title}</div>
      <ul class="flex flex-col gap-0.5">
        {#each tags as lead (lead.tag)}
          <li class="flex justify-between gap-2">
            <span class="truncate">{lead.tag}</span>
            <span class="shrink-0 opacity-60">×{lead.factor.toFixed(1)} · {Math.round(lead.views)} views</span>
          </li>
        {/each}
      </ul>
    </li>
  {/if}
{/snippet}

<SettingsGroup title="🎣 Background fetching">
  <li class={NOTE}>How often posts fetched for your tastes turn out to be ones you linger on or like, compared with TikTok overall.</li>
  {#if stats.judged === 0}
    <li class={NOTE}>No judged views of fetched posts yet. Scroll TikTok for a while.</li>
  {:else}
    <li class="grid grid-cols-2 gap-2 px-3 py-2.5 text-center">
      <div>
        <div class="text-2xl font-semibold">{percent(stats.fetchedRate)}</div>
        <div class="text-xs opacity-60">hit rate of fetched posts</div>
      </div>
      <div>
        <div class="text-2xl font-semibold">{percent(stats.overallRate)}</div>
        <div class="text-xs opacity-60">all TikTok posts</div>
      </div>
    </li>
    {@render leads('Fetches that pay off', stats.best)}
    {@render leads('Fetches that miss', stats.worst)}
  {/if}
  <li class={[NOTE, 'opacity-60']}>{stats.fetched.toLocaleString()} posts brought in by taste fetches.</li>
</SettingsGroup>
