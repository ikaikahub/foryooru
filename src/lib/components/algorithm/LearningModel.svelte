<script lang="ts">
  import type { ModelStatus } from '$lib/api';
  import SettingsGroup from '../SettingsGroup.svelte';
  import { NOTE, percent } from './stats';

  let { status }: { status: ModelStatus } = $props();
</script>

<SettingsGroup title="🧠 Learning model">
  <li class={NOTE}>
    Predicts whether you'll linger on or like each TikTok post, and reorders TikTok only while it predicts better than the
    regular ranking.
  </li>
  {#if status.graded < status.needed}
    <li class={NOTE}>Still learning: {status.graded} of {status.needed} judged views before it can take over.</li>
  {:else}
    <li class="grid grid-cols-3 gap-2 px-3 py-2.5 text-center">
      <div>
        <div class="text-2xl font-semibold">{percent(status.model)}</div>
        <div class="text-xs opacity-60">model accuracy</div>
      </div>
      <div>
        <div class="text-2xl font-semibold">{percent(status.current)}</div>
        <div class="text-xs opacity-60">regular ranking</div>
      </div>
      <div>
        <div class="text-2xl font-semibold">{Math.round(status.trust * 100)}%</div>
        <div class="text-xs opacity-60">in use</div>
      </div>
    </li>
    <li class={[NOTE, 'opacity-60']}>
      Over your last {status.graded} judged TikTok views. 50% is guessing, 100% is perfect. It takes over fully once it's 5 points ahead.
    </li>
  {/if}
  <li class={[NOTE, 'opacity-60']}>Learned from {status.learned.toLocaleString()} views so far.</li>
</SettingsGroup>
