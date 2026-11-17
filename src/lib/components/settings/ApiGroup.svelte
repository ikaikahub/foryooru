<script lang="ts">
  import { api, errorMessage, type Site } from '$lib/api';
  import { SITES, currentSite } from '$lib/sites';
  import { settings } from '$lib/state/settings.svelte';
  import SelectMenu from '../SelectMenu.svelte';
  import SettingsGroup from '../SettingsGroup.svelte';
  import SettingsRow from '../SettingsRow.svelte';

  const NOTE = 'px-3 py-2.5 text-sm';
  const SITE_OPTIONS = (Object.keys(SITES) as Site[]).map((site) => ({ value: site, label: SITES[site].label }));

  let apiError = $state<string>();
  let credentials = $state(settings.apiKey ? `&api_key=${settings.apiKey}&user_id=${settings.userId}` : '');

  const parsed = $derived.by(() => {
    const params = new URLSearchParams(credentials.trim().replace(/^[?&]/, ''));
    const [apiKey, userId] = [params.get('api_key'), params.get('user_id')];
    return apiKey && userId ? { apiKey, userId } : null;
  });
  const incomplete = $derived(credentials.trim() !== '' && !parsed);

  function saveCredentials() {
    if (!credentials.trim()) [settings.apiKey, settings.userId] = ['', ''];
    else if (parsed) [settings.apiKey, settings.userId] = [parsed.apiKey, parsed.userId];
  }

  let checkingPool = $state(false);
  let poolDetail = $state('Optional: recommend from the full Kimochii pool running on your computer. Leave the address empty to recommend from posts fetched here.');

  async function checkPool() {
    checkingPool = true;
    try {
      const status = await api.kimochiiStatus();
      poolDetail = `Connected: ${status.posts.toLocaleString()} posts.`;
    } catch (error) {
      poolDetail = errorMessage(error);
    } finally {
      checkingPool = false;
    }
  }

  async function switchSite(site: Site) {
    apiError = undefined;
    try {
      await api.setSite(site);
      settings.site = site;
      location.reload();
    } catch (error) {
      apiError = errorMessage(error);
    }
  }

</script>

<SettingsGroup title="API settings">
  <SettingsRow icon="🌐" label="Site" detail="Each site keeps its own likes, blacklist and history.">
    <SelectMenu value={settings.site} options={SITE_OPTIONS} buttonClass="btn-sm w-28" alignEnd onchange={switchSite} />
  </SettingsRow>
  {#if settings.site === 'gelbooru'}
    <SettingsRow icon="🛰️" label="Proxy">
      <input
        class={['input input-sm w-44 font-mono', !settings.gelbooruProxy.trim() && 'input-warning']}
        autocomplete="off"
        spellcheck="false"
        placeholder="https://….workers.dev"
        aria-label="Gelbooru proxy address"
        bind:value={settings.gelbooruProxy}
      />
    </SettingsRow>
    <li class={[NOTE, 'opacity-70']}>
      Browsers can't reach Gelbooru directly, so searches and images go through your own Cloudflare Worker (see proxy/ in
      the project).
    </li>
    <SettingsRow icon="🔑" label="API key">
      <input
        class={['input input-sm w-44 font-mono', incomplete && 'input-error']}
        autocomplete="off"
        spellcheck="false"
        placeholder="&api_key=…&user_id=…"
        aria-label="Gelbooru API credentials"
        bind:value={credentials}
        oninput={saveCredentials}
      />
    </SettingsRow>
    <li class={[NOTE, incomplete ? 'text-error' : 'opacity-70']}>
      {incomplete
        ? 'That needs both api_key and user_id. Paste the whole line.'
        : 'Gelbooru → My Account → Options → API Access Credentials. Paste the whole line.'}
    </li>
    <SettingsRow icon="🖥️" label="Pool server">
      <input
        class="input input-sm w-44 font-mono"
        autocomplete="off"
        spellcheck="false"
        placeholder="http://192.168.1.10:8790"
        aria-label="Kimochii server address"
        bind:value={settings.kimochiiUrl}
      />
    </SettingsRow>
    <SettingsRow icon="📡" label="Connection" detail={poolDetail}>
      <button class="btn btn-sm" disabled={checkingPool} onclick={checkPool}>Check</button>
    </SettingsRow>
  {:else}
    <SettingsRow icon="👤" label="Username">
      <input class="input input-sm w-36" autocomplete="off" placeholder="Optional" aria-label="Danbooru username" bind:value={settings.danbooruLogin} />
    </SettingsRow>
    <SettingsRow icon="🔑" label="API key">
      <input
        class="input input-sm w-36 font-mono"
        type="password"
        autocomplete="off"
        placeholder="Optional"
        aria-label="Danbooru API key"
        bind:value={settings.danbooruApiKey}
      />
    </SettingsRow>
    <li class={[NOTE, 'opacity-70']}>Optional: without them Danbooru works anonymously, with up to 2 tags per search.</li>
    <!-- Gelbooru favourites need its cookie login, which only the desktop app has -->
    <SettingsRow
      icon="🔄"
      label="Sync favourites"
      detail="Liking a post also favourites it on your {currentSite().label} account, and unliking removes it. Needs your username and API key."
    >
      <input type="checkbox" class="toggle toggle-sm" aria-label="Sync favourites" bind:checked={settings.syncFavourites} />
    </SettingsRow>
  {/if}
  {#if apiError}<li class={[NOTE, 'text-error']}>{apiError}</li>{/if}
</SettingsGroup>
