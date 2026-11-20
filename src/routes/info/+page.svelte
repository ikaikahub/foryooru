<script lang="ts">
  import GroupColumns from '$lib/components/GroupColumns.svelte';
  import SettingsGroup from '$lib/components/SettingsGroup.svelte';
  import { RELEASES_API, type Release } from '$lib/releases';
  import { onMount } from 'svelte';

  const CHANGELOG_RELEASES = 30;

  let releases = $state<Release[] | null>(null);
  let changelogError = $state(false);

  onMount(() => {
    fetch(`${RELEASES_API}?per_page=${CHANGELOG_RELEASES}`)
      .then((response) => (response.ok ? response.json() : Promise.reject(response.status)))
      .then(
        (found: Release[]) => (releases = found),
        () => (changelogError = true),
      );
  });

  type Keybind = { keys: string[]; action: string };

  const KEYBINDS: { title: string; binds: Keybind[] }[] = [
    {
      title: 'Post viewer',
      binds: [
        { keys: ['←', '→'], action: 'Previous / next post' },
        { keys: ['F'], action: 'Like' },
        { keys: ['D'], action: 'Dislike' },
        { keys: ['Esc'], action: 'Close' },
      ],
    },
    {
      title: 'TikTok',
      binds: [
        { keys: ['↓', 'J', 'Page Down'], action: 'Next post' },
        { keys: ['↑', 'K', 'Page Up'], action: 'Previous post' },
        { keys: ['F'], action: 'Like' },
        { keys: ['D'], action: 'Dislike' },
        { keys: ['I'], action: 'Show / hide the sidebar' },
        { keys: ['Esc'], action: 'Close a menu, or leave TikTok' },
      ],
    },
    {
      title: 'Post grids',
      binds: [{ keys: ['←', '→'], action: 'Previous / next page' }],
    },
    {
      title: 'Mouse',
      binds: [
        { keys: ['Click'], action: 'Open a post in the viewer' },
        { keys: ['Ctrl + Click'], action: 'Open a post on the site, in your browser' },
        { keys: ['Middle-click'], action: 'Open a post on the site (Linux only)' },
        { keys: ['Right-click'], action: 'Post menu: copy or save (long-press on Android)' },
        { keys: ['Click a tag'], action: 'Tag menu: include, exclude, favourite, blacklist' },
        { keys: ['Back', 'Forward'], action: 'Mouse side buttons: go back / forward' },
        { keys: ['Drag'], action: 'Resize the sidebar by its edge' },
        { keys: ['Double-click'], action: 'Reset the sidebar width (on its edge)' },
      ],
    },
    {
      title: 'Search box',
      binds: [
        { keys: ['↑', '↓'], action: 'Pick a suggestion' },
        { keys: ['Enter'], action: 'Add the tag, or search' },
        { keys: ['Backspace'], action: 'Remove the last tag (when the box is empty)' },
        { keys: ['Esc'], action: 'Close suggestions' },
      ],
    },
  ];
</script>

<div class="px-2 pt-2 pb-6">
  <GroupColumns>
    <SettingsGroup title="Foryooru">
      <li class="px-3 py-2.5 text-sm">Foryooru is a whatever placeholder text blah blah ill write it later</li>
    </SettingsGroup>

    <SettingsGroup title="Changelog">
      {#if changelogError}
        <li class="px-3 py-2.5 text-sm opacity-70">Couldn't reach GitHub to load the changelog.</li>
      {:else if !releases}
        <li class="px-3 py-2.5"><span class="loading loading-spinner loading-sm"></span></li>
      {:else}
        {#each releases as release (release.tag_name)}
          <li class="px-3 py-2.5 text-sm">
            <div class="flex items-baseline justify-between gap-3">
              <span class="font-semibold">{release.tag_name}</span>
              <span class="text-xs opacity-60">{new Date(release.published_at).toLocaleDateString()}</span>
            </div>
            {#if release.body?.trim()}
              <p class="mt-1 whitespace-pre-line opacity-80">{release.body.trim()}</p>
            {/if}
          </li>
        {/each}
      {/if}
    </SettingsGroup>

    {#each KEYBINDS as group (group.title)}
      <SettingsGroup title="Keybinds: {group.title}">
        {#each group.binds as bind (bind.action)}
          <li class="flex items-center justify-between gap-3 px-3 py-2 text-sm">
            <span>{bind.action}</span>
            <span class="flex shrink-0 gap-1">
              {#each bind.keys as key (key)}<kbd class="kbd kbd-sm">{key}</kbd>{/each}
            </span>
          </li>
        {/each}
      </SettingsGroup>
    {/each}
  </GroupColumns>
</div>
