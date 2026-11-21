<script lang="ts">
  import { api, type Suggestion, type Tag } from '$lib/api';
  import { looseTag, staged, type TagEditor } from '$lib/state/search.svelte';
  import { tagText } from '$lib/tags';

  let {
    editor = staged,
    placeholder = 'Add a tag…',
    onsubmit,
    onfocus,
  }: { editor?: TagEditor; placeholder?: string; onsubmit: () => void; onfocus?: () => void } = $props();

  // the Artists page's filter menu shows a second one
  const listId = $props.id();

  const DEBOUNCE_MS = 150;

  let suggestions = $state<Suggestion[]>([]);
  let active = $state(-1);
  let timer: ReturnType<typeof setTimeout>;
  let field = $state<HTMLInputElement>();

  const sign = $derived(editor.typed.trim().startsWith('-') ? '-' : '');

  function close() {
    suggestions = [];
    active = -1;
  }

  function oninput() {
    clearTimeout(timer);
    const typed = editor.typed.trim().replace(/^-/, '');
    if (!typed) return close();
    // answers for text that has since changed or been picked are dropped
    const stillTyped = () => editor.typed.trim().replace(/^-/, '') === typed;
    timer = setTimeout(async () => {
      // stored tags show straight away, then the site's own autocomplete adds the ones they're missing
      const found = await api.suggestTags(typed);
      if (!stillTyped()) return;
      suggestions = found;
      active = -1;
      const online = await api.suggestTagsOnline(typed).catch(() => []);
      if (!stillTyped()) return;
      const shown = new Set(suggestions.map((tag) => tag.name));
      suggestions = [...suggestions, ...online.filter((tag) => !shown.has(tag.name))];
    }, DEBOUNCE_MS);
  }

  function pick(tag: Tag) {
    editor.add(sign + tag.name, tag.type);
    close();
  }

  const looseMatch = () => suggestions.find((tag) => looseTag(tag.name) === looseTag(editor.typed.replace(/^-/, '')));

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      if (!suggestions.length) return;
      e.preventDefault();
      active = (active + (e.key === 'ArrowDown' ? 1 : -1) + suggestions.length) % suggestions.length;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const chosen = suggestions[active] ?? looseMatch();
      if (chosen) pick(chosen);
      else if (editor.typed.trim()) {
        editor.commitTyped();
        close();
      } else onsubmit();
    } else if (e.key === 'Escape') {
      close();
    } else if (e.key === 'Backspace' && !editor.typed && editor.chips.length) {
      editor.chips = editor.chips.slice(0, -1);
    }
  }

  export function blur() {
    field?.blur();
  }

  export function focus() {
    field?.focus();
  }

  export function commit() {
    const chosen = looseMatch();
    if (chosen) pick(chosen);
    else editor.commitTyped();
    close();
  }
</script>

<div class="relative">
  <input
    bind:this={field}
    class="input input-sm w-full"
    {placeholder}
    autocomplete="off"
    autocapitalize="none"
    spellcheck="false"
    role="combobox"
    aria-expanded={suggestions.length > 0}
    aria-controls={listId}
    bind:value={editor.typed}
    {oninput}
    {onkeydown}
    onblur={close}
    {onfocus}
  />
  {#if suggestions.length}
    <ul
      id={listId}
      role="listbox"
      class="menu menu-sm absolute inset-x-0 top-full z-(--z-dropdown) max-h-72 flex-nowrap overflow-y-auto rounded-b-box border border-base-300 bg-base-100 p-0 shadow"
      onmousedown={(e) => e.preventDefault()}
    >
      {#each suggestions as tag, index (tag.name)}
        <li role="option" aria-selected={index === active}>
          <button type="button" class={['flex justify-between', index === active && 'menu-active']} onclick={() => pick(tag)}>
            <span class="break-all">
              {#if tag.alias}<span class="opacity-60">{tag.alias} → </span>{/if}<span class={tagText(tag.type)}>{sign}{tag.name}</span>
            </span>
            {#if tag.count !== null}<small class="opacity-60">{tag.count.toLocaleString()}</small>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
