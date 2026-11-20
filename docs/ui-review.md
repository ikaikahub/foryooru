# UI, loading and UX review

## Status

Done:
- Images fade in, and viewer and TikTok media show the cached thumbnail until the real image paints (section 2).
- The viewer renders media straight from the grid's post; the tag panel keeps the previous post (dimmed) instead of unmounting, and shows tag loading errors (section 2).
- One `swipeAxis` helper in `motion.ts` for the viewer, TikTok, side menu, info sheet and pull to refresh (section 3).
- `LoadingBar` and `Toast` animate in; grids no longer dim on reruns; TikTok's first load uses `LoadingBar`; "Loading more…" has a spinner; "Fetching..." became "Searching…" (sections 1 and 2).
- The ↑/↓ buttons are hidden on mobile, and Discover is an icon-only toggle in the mobile top bar (section 4).
- Zoom snaps back below 1.15×; the related-posts similar fallback ranks only 20 posts (sections 3 and 7).

Still open: skeleton grid for first loads, safe areas, info sheet snap height, one motion style for sheets, returning posts with the For You ids, the timing checks in section 7, the structural cleanups in section 5, and the empty state, error and toast polish in section 8.

A pass over the frontend (and the backend calls behind the slow screens), looking for jank: things that pop in, inconsistent loading, gestures that fight each other, and features that were bolted on without the rest of the UI in mind. Ordered roughly by how much they'd improve how the app feels.

## 1. Loading is shown five different ways

Today the app has:

| Where | What it shows |
|---|---|
| Grids, For You, Activity, Algorithm, Database, Settings | `LoadingBar`: a fixed pill under the top bar, plus a 3px bar |
| TikTok first load | A centred "Finding posts…" overlay with a small spinner (`Feed.svelte`) |
| TikTok next batch | A full-height page with a large spinner |
| TikTok video or gif | A large spinner over the media (`FeedItem.svelte`) |
| Viewer while details load | A default spinner, alone in the middle (`PostViewer.svelte`) |
| Related posts | A medium, faded spinner in a 100px box |
| Infinite scroll in For You | Plain "Loading more…" text, no spinner (`RankedView.svelte`) |
| Pull to refresh | `PullIndicator`, then the `LoadingBar` pops in separately |

The wording varies too: "Fetching...", "Loading…", "Finding recommendations…", "Finding posts…".

**Suggestion:** pick two patterns and use them everywhere.
- **Page-level work** (a new search, a rerun, settings actions): `LoadingBar`, with one wording style ("Searching…", "Ranking…").
- **Something filling a known space** (grid, viewer slide, feed slide, related strip, "load more"): a placeholder of the right size, see the next section.

Also give `LoadingBar` and `Toast` a short fade or slide in and out; both currently pop.

## 2. Show what we already have instead of spinners

This is the biggest perceived-speed win. The app usually already knows a post's size and has its thumbnail cached, but it still shows a spinner or a blank.

- **Viewer opens to a spinner.** `PostViewer` only renders once `post_detail` comes back (`details.get(...)`), even though the grid already passed in a `Post` with dimensions, and `post_detail` may do a network call (`media::complete_partial`). The media could render immediately from the `Post` the grid already has; only the tag panel needs to wait for details.
- **The tag panel blinks on every post switch.** `post` is `null` while details load, so `{#if post && …}` unmounts the whole `<aside>`, including related posts. Keep the previous post's tags on screen (dimmed) until the new ones arrive, or render the panel with a skeleton.
- **Images pop in at full size.** The grid has a correctly-sized `bg-base-300` box (good, no layout shift), but the image appears abruptly. The viewer and TikTok show black until the sample loads. Use the cached thumbnail as a blurred placeholder, swap to the sample when it decodes (`img.decode()`), and fade over 120–150ms. The same applies to the related-posts strip.
- **TikTok images have no loading state at all.** `mediaReady` treats images as always ready, so a slow sample shows a black frame while videos show a spinner. The thumbnail placeholder above fixes both consistently.
- **First page of For You or Browse is an empty page plus a pill.** Render a skeleton grid (grey boxes in a few column heights) during the first load.
- **Reruns dim the whole grid to 35% and block it** (`ListingView`, `RankedView`). On a phone this looks like a freeze. Keep the old grid at full opacity with the top bar's progress line, and swap once the new data is ready.

## 3. Gestures: three slightly different implementations

Swipe direction is decided differently in different places:

- **`Feed.svelte` and `SideMenu.svelte`:** wait for 8px (`AXIS_LOCK_PX`) before deciding. Correct.
- **`PullToRefresh.svelte` (`pull.mode ??= …`) and `InfoSheet.svelte` (`drag.mode ??= …`):** decide on the first touch move, so a slightly sideways start kills the gesture. This is the same bug that made TikTok sometimes refuse to scroll.

**Suggestion:** one small helper in `motion.ts`, for example `lockAxis(start, touch)` that returns `'x' | 'y' | null`, used by all four. That fixes the two remaining bugs and stops this from coming back.

Related:
- **Pull to refresh only exists on `/`, `/for-you`, `/likes` and `/disliked`.** TikTok has its own pull inside `Feed.svelte`. Check that the threshold and feel match.
- **Accidental zoom:** a slightly zoomed image (zoom over 1.05) in TikTok or the viewer swallows swipes. Reset zoom to 1 when a pinch ends below about 1.15, not just below 1.05.

## 4. Mobile layout and chrome

- **The ↑ / ↓ floating buttons show on mobile** (`+layout.svelte`). They sit over the grid's bottom-right reaction buttons, and the top bar already hides on scroll. Hide them in the drawer layout, or show only ↑ after scrolling a screen or more.
- **The top bar is crowded on algorithm pages.** The search field, ☰ and the Discover button share 56px. "🧭 Discover ✓" squeezes the search box. Make it an icon-only toggle on mobile (🧭, filled when on), or move it into the search sheet next to the filters.
- **The search sheet and the top bar animate differently.** The sheet flies in, the backdrop fades and the icon scales, while the side menu uses springs. Pick one motion style (springs or 180ms ease-out) for every sheet.
- **Info sheet height is fixed** (65% in TikTok, `SHEET_SHARE` in the viewer). With related posts added, the tags start below the fold. Consider a snap point at about 90%, or making the strip more compact on mobile (h-20 thumbnails).
- **Safe areas:** check the fixed bottom bars (the viewer actions and the TikTok side buttons) against gesture-nav phones: nothing in the app uses `env(safe-area-inset-*)` yet, and they're positioned with plain `bottom-2`.

## 5. Features that don't know about each other

- **Related posts were added inside `PostTags`,** so they show anywhere tags show, but each host behaves differently. The viewer swaps the post, TikTok inserts it into the feed, and the side panel on desktop TikTok does the TikTok thing. That's right, but it's wired through three components with an `onrelated` prop. A small "post navigation" context (provided by the viewer or the feed, read by `PostFamily`) would remove the prop threading.
- **`library.remember` is called from six places** (grid listing, feed, viewer, related, ranked view, page turning) to keep like and dislike state in sync. It's easy to forget in new code. Making `api.posts`, `api.postDetail` and `api.feedNext` results flow through one wrapper that remembers them would make it automatic.
- **Two ways to show a post outside a grid:** `PostMedia` (viewer, feed) and the grid's own `<img>`/`<video>` code in `PostGrid`. The retry, visibility and animated-gif logic is duplicated with small differences. The thumbnail placeholder work in section 2 is a good moment to merge them into one media component with a `fit` mode.
- **The "seen" tracking rules differ:** the grid counts an image as seen when visible (`seen()` attachment), TikTok uses its own dwell clock, and the viewer adds an impression on open. That's fine, but the thresholds live in three files. Worth a single `impressions.ts` policy so tuning one doesn't silently diverge.

## 6. Two-step loading that could be one

- **For You:** `for_you` returns ids, then `RankedView` calls `api.posts` for the first 100. That's two round trips before anything shows. Return the first page's `Post`s together with the ids.
- **Viewer:** `post_detail` (tags) is fetched separately from the media the grid already has, see section 2. With media rendering from the grid's `Post`, the detail call only blocks the tag panel.
- **Related posts:** the first open of an old post may make one extra `id:<post>` request to learn its group. That's fine, but it happens while the panel shows a spinner. Start it as soon as the viewer opens (it's independent of `post_detail`).

## 7. Backend hot paths to measure

These aren't confirmed slow, but they're the ones that grew recently and run on every page:

- **For You re-ranks fully on every 100-post page** (the "fresh while scrolling" change). With per-like interests under 30 likes, that's up to 29 full corpus scorings per page. Time `for_you_more` on the phone. If it's over about 150ms, cache the interest group profiles between pages (they only change when likes or the session change).
- **`sort_lean` and `adjustments` build a `HashMap` over every candidate** on each call, and TikTok calls it for every batch. Fine at 10k posts; check at 100k.
- **`post_family` similar fallback** runs a full `recommend::similar` (up to `RANKING_LIMIT` = 2000) to keep 10. Use `corpus.top(…, 20, …)` instead of `rank`.
- **`corpus.get()` appends new posts under the cache lock** while background fetches insert. Long appends block every command that needs the corpus, which can look like a frozen feed. Log how long `append` takes.

## 8. Smaller polish

- **Empty states:** "No posts found." / "Nothing matches these filters yet." / "No recommendations match those tags." Use one tone, and offer an action (clear filters, broaden).
- **`ErrorNote` placement varies** (centred, inline, top). Errors from a rerun replace the grid only when there's no data; otherwise they sit above the stale grid. Decide on one behaviour.
- **The Discover toggle drops everything after the current TikTok post** and refetches, which shows the next-batch spinner. Fine, but a toast ("Discover on, new posts from here") would make the delay make sense.
- **The `Toast` has no queue animation;** two toasts in a row replace each other abruptly.

## Suggested order

1. Thumbnail-first media (viewer opens instantly; images fade in; TikTok shows placeholders). This fixes most "doesn't load in naturally".
2. Keep the viewer tag panel mounted between posts.
3. One axis-lock helper for all gestures.
4. Unify loading indicators, and stop dimming on reruns.
5. Mobile chrome: floating buttons, Discover in the top bar, safe areas.
6. Return posts with the For You ids, and the measurements in section 7.
7. The structural cleanups in section 5, ideally alongside 1 since they touch the same components.
