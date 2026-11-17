<script lang="ts">
  import { api } from '$lib/api';
  import ReactedPosts from '$lib/components/ReactedPosts.svelte';
</script>

<ReactedPosts title="❤️ Liked posts" emptyMessage="You haven't liked any posts yet." load={api.likedPosts} />
