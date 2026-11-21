<script lang="ts">
  import { api, type Suggestion, type Tag } from '$lib/api';
  import { bare, looseTag, staged, type TagEditor } from '$lib/state/search.svelte';
  import { tagText } from '$lib/tags';
  import TagChip from './TagChip.svelte';

  let {
    editor = staged,
    placeholder = 'Add a tag…',
    inlineChips = false,
    onsubmit,
    onfocus,
  }: {
    editor?: TagEditor;
    placeholder?: string;
    // the chips sit in the box on one line that scrolls sideways, instead of being shown somewhere else
    inlineChips?: boolean;
    onsubmit: () => void;
    onfocus?: () => void;
  } = $props();

  // the Artists page's filter menu shows a second one
  const listId = $props.id();

  const DEBOUNCE_MS = 150;

  let suggestions = $state<Suggestion[]>([]);
  let active = $state(-1);
  let timer: ReturnType<typeof setTimeout>;
  let field = $state<HTMLInputElement>();
  let chipRow = $state<HTMLElement>();

  // a new chip pushes the text field right, so the row follows it to keep the field in view
  $effect(() => {
    void editor.chips.length;
    if (chipRow) chipRow.scrollLeft = chipRow.scrollWidth;
  });

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
  {#if inlineChips}
    <!-- a tap on the box's empty space goes to the text field -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      bind:this={chipRow}
      class="input input-sm no-scrollbar flex w-full flex-nowrap items-center gap-1 overflow-x-auto"
      onclick={() => field?.focus()}
    >
      {#each editor.chips as chip (chip.tag)}
        <span class="shrink-0">
          <TagChip tag={chip.tag} type={chip.type} title="Include / exclude" onclick={() => editor.toggleExclude(chip)}>
            <button
              type="button"
              class="flex min-w-6 cursor-pointer items-center justify-center self-stretch pr-1 opacity-60 hover:opacity-100"
              aria-label="Remove"
              onmousedown={(e) => e.preventDefault()}
              onclick={(e) => {
                e.stopPropagation();
                editor.remove(bare(chip.tag));
              }}>×</button
            >
          </TagChip>
        </span>
      {/each}
      {@render fieldInput('min-w-24 flex-1 border-none bg-transparent p-0 outline-none')}
    </div>
  {:else}
    {@render fieldInput('input input-sm w-full')}
  {/if}
  {#snippet fieldInput(fieldClass: string)}
  <input
    bind:this={field}
    class={fieldClass}
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
  {/snippet}
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
