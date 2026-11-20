<script lang="ts">
  import BottomSheet from './BottomSheet.svelte';

  let { open = $bindable() }: { open: boolean } = $props();

  const SIGNALS: { signal: string; effect: string; note: string }[] = [
    { signal: '❤️ Like', effect: '+1', note: "Will show more posts like it" },
    { signal: '💔 Dislike', effect: '−0.5', note: "Will show less posts like it" },
    { signal: '👁️ Opened in the viewer', effect: '+0.3', note: 'Counted up to twice per post' },
    { signal: '🔁 Seen (grid or TikTok)', effect: '×0.7', note: "Once per post, doesn't affect interests but will be less likely to show it again. TikTok views under 0.4s don't count" },
    { signal: '🚫 Blacklisted tag', effect: 'hidden', note: "Hides a post" },
  ];
</script>

<BottomSheet bind:open title="What affects recommendations">
  <div class="flex flex-col gap-3 text-sm">
    <div class="overflow-x-auto">
      <table class="table table-sm">
        <tbody>
          <tr>
            <th>Action</th>
            <th>Weight</th>
            <th>Effect</th>
          </tr>
          {#each SIGNALS as row (row.signal)}
          <tr>
              <td class="whitespace-nowrap">{row.signal}</td>
              <td class="font-mono whitespace-nowrap">{row.effect}</td>
              <td class="opacity-80">{row.note}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</BottomSheet>
