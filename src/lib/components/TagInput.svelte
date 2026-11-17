<script lang="ts">
  import { api, type Tag } from '$lib/api';
  import { looseTag, staged, type TagEditor } from '$lib/state/search.svelte';
  import { tagText } from '$lib/tags';

  let {
    editor = staged,
    onsubmit,
    onfocus,
  }: { editor?: TagEditor; onsubmit: () => void; onfocus?: () => void } = $props();

  const DEBOUNCE_MS = 150;

  let suggestions = $state<Tag[]>([]);
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
    timer = setTimeout(async () => {
      const found = await api.suggestTags(typed);
      if (editor.typed.trim()) {
        suggestions = found;
        active = -1;
      }
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
    placeholder="Add a tag…"
    autocomplete="off"
    autocapitalize="none"
    spellcheck="false"
    role="combobox"
    aria-expanded={suggestions.length > 0}
    aria-controls="tag-suggestions"
    bind:value={editor.typed}
    {oninput}
    {onkeydown}
    onblur={close}
    {onfocus}
  />
  {#if suggestions.length}
    <ul
      id="tag-suggestions"
      role="listbox"
      class="menu menu-sm absolute inset-x-0 top-full z-(--z-dropdown) max-h-72 flex-nowrap overflow-y-auto rounded-b-box border border-base-300 bg-base-100 p-0 shadow"
      onmousedown={(e) => e.preventDefault()}
    >
      {#each suggestions as tag, index (tag.name)}
        <li role="option" aria-selected={index === active}>
          <button type="button" class={['flex justify-between', index === active && 'menu-active']} onclick={() => pick(tag)}>
            <span class={['break-all', tagText(tag.type)]}>{sign}{tag.name}</span>
            {#if tag.count !== null}<small class="opacity-60">{tag.count.toLocaleString()}</small>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
