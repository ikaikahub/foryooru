<script lang="ts">
  import { api, errorMessage, type Site } from '$lib/api';
  import { SITES } from '$lib/sites';
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
    saveSession();
  }

  // the login cookie is user_id plus pass_hash; user_id is the same one as in the API key line
  let passHash = $state(settings.gelbooruSession?.passHash ?? '');

  function saveSession() {
    const hash = passHash.trim();
    settings.gelbooruSession = hash && settings.userId ? { userId: settings.userId, passHash: hash } : null;
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
    <SettingsRow icon="🍪" label="Login cookie">
      <input
        class="input input-sm w-44 font-mono"
        autocomplete="off"
        spellcheck="false"
        placeholder="Optional"
        aria-label="Gelbooru pass_hash cookie"
        bind:value={passHash}
        oninput={saveSession}
      />
    </SettingsRow>
    <li class={[NOTE, 'opacity-70']}>
      Optionally provide the value of the "pass_hash" cookie so that new favourites will sync. Must be from the same account as the api key.
    </li>
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
    <li class={[NOTE, 'opacity-70']}>Optionally provide an api key for syncing new favourite and increased limits (if youre a paying member).</li>
  {/if}
  {#if apiError}<li class={[NOTE, 'text-error']}>{apiError}</li>{/if}
</SettingsGroup>
