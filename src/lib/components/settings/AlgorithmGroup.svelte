<script lang="ts">
  import { settings } from '$lib/state/settings.svelte';
  import SettingsGroup from '../SettingsGroup.svelte';
  import SettingsRow from '../SettingsRow.svelte';

  function setPerArtist(typed: string) {
    const count = Math.floor(Number(typed));
    settings.similarPerArtist = typed.trim() && count >= 1 ? count : null;
  }
</script>

<SettingsGroup title="All presets">
  <SettingsRow icon="♥" label="Hide Likes" detail="Posts you've liked won't show up in algorithm searches">
    <input type="checkbox" class="toggle toggle-sm" aria-label="Hide favourites" bind:checked={settings.hideLiked} />
  </SettingsRow>
  <SettingsRow icon="🤵" label="Posts per artist" detail="Max posts by an artist / 100 posts in algorithm searches. Leave empty for no limit.">
    <input
      type="number"
      min="1"
      step="1"
      class="input input-sm w-28"
      placeholder="Unlimited"
      aria-label="Similar: posts per artist"
      value={settings.similarPerArtist ?? ''}
      oninput={(e) => setPerArtist(e.currentTarget.value)}
    />
  </SettingsRow>
</SettingsGroup>
