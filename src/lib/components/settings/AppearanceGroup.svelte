<script lang="ts">
  import { MAX_COLUMNS, settings } from '$lib/state/settings.svelte';
  import { THEMES } from '$lib/theme';
  import SelectMenu from '../SelectMenu.svelte';
  import SettingsGroup from '../SettingsGroup.svelte';
  import SettingsRow from '../SettingsRow.svelte';

  const THEME_OPTIONS = THEMES.map((theme) => ({ value: theme, label: theme.charAt(0).toUpperCase() + theme.slice(1) }));

  const COLUMNS = [
    { value: 0, label: 'Auto' },
    ...Array.from({ length: MAX_COLUMNS }, (_, index) => ({ value: index + 1, label: String(index + 1) })),
  ];
</script>

<SettingsGroup title="Appearance">
  <SettingsRow icon="🎨" label="Theme">
    <SelectMenu bind:value={settings.theme} options={THEME_OPTIONS} buttonClass="btn-sm w-28" alignEnd />
  </SettingsRow>
  <SettingsRow icon="🔲" label="Grid columns">
    <SelectMenu bind:value={settings.columns} options={COLUMNS} buttonClass="btn-sm w-28" alignEnd />
  </SettingsRow>
  <SettingsRow icon="🎞️" label="Autoplay in grid" detail="GIFs and videos in the grid play muted while they're on screen.">
    <input type="checkbox" class="toggle toggle-sm" aria-label="Autoplay in grid" bind:checked={settings.gridAutoplay} />
  </SettingsRow>
</SettingsGroup>
