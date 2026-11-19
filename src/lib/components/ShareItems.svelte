<script lang="ts">
  import type { Post, Site } from '$lib/api';
  import { copyImage, copyImageLink, copyPageLink, copyPostLink, openPostLink, saveOriginal, saveSample } from '$lib/postActions';
  import { settings } from '$lib/state/settings.svelte';
  import type { ClassValue } from 'svelte/elements';
  import ActionButton from './ActionButton.svelte';

  let { post, site = settings.site, class: className, iconEnd = false }: { post: Post; site?: Site; class?: ClassValue; iconEnd?: boolean } = $props();
</script>
<li><ActionButton class={className} {iconEnd} icon="↗" label="Open original url" doneLabel="Copied" action={() => openPostLink(post, site)} /></li>
<li><ActionButton class={className} {iconEnd} icon="🌐" label="Copy post address" doneLabel="Copied" action={() => copyPostLink(post, site)} /></li>
<li><ActionButton class={className} {iconEnd} icon="📎" label="Copy Foryooru link" doneLabel="Copied" action={() => copyPageLink(post, site)} /></li>
<li><ActionButton class={className} {iconEnd} icon="🔗" label="Copy media address" doneLabel="Copied" action={() => copyImageLink(post, site)} /></li>
{#if post.media !== 'video'}
  <li><ActionButton class={className} {iconEnd} icon="📋" label="Copy image" doneLabel="Copied" action={() => copyImage(post, site)} /></li>
{/if}
{#if post.media === 'image'}
  <li><ActionButton class={className} {iconEnd} icon="📥" label="Download sample" doneLabel="Downloaded" action={() => saveSample(post, site)} /></li>
{/if}
<li><ActionButton class={className} {iconEnd} icon="💾" label="Download original" doneLabel="Downloaded" action={() => saveOriginal(post, site)} /></li>
