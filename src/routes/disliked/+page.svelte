<script lang="ts">
  import { api } from '$lib/api';
  import ReactedPosts from '$lib/components/ReactedPosts.svelte';
</script>

<ReactedPosts title="💔 Disliked posts" emptyMessage="You haven't disliked any posts." load={api.dislikedPosts} />
