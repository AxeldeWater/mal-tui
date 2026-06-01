# Related Timeline Screen — Design

**Date:** 2026-06-01
**Status:** Approved (design), pending implementation plan

## Summary

Add a new top-level screen, **"Related"**, to the mal-tui app. The user types an
anime title, picks from the search matches, and the screen builds a horizontal
**watch-order timeline**: prequels and sequels laid out left-to-right as the
main line, with other relation types (side story, alternative version, spin-off,
summary, etc.) shown as branches off the selected node. Selecting any node opens
the existing anime detail popup.

## Goals

- Let a user explore how an anime relates to its prequels/sequels/side stories.
- Present the prequel→sequel chain as an intuitive left-to-right watch order.
- Reuse existing infrastructure (search, anime store, popup overlay, navbar,
  screen caching, image manager) rather than building parallel systems.

## Non-goals

- Manga relations (`related_manga`) — out of scope for this screen.
- Editing/managing relations (data is read-only from MAL).
- Re-rooting the timeline by selecting a node (selected nodes open the popup
  instead). Could be a future enhancement.

## User flow

1. User opens the **Related** screen from the navbar.
2. **Search mode:** user types a title into a search input and submits.
3. **Picking mode:** the screen shows matching anime from MAL search; user
   selects one as the timeline root.
4. **Timeline mode:** the screen builds and displays the watch-order timeline
   for the chosen anime.
5. Selecting a node (on the line or in a branch) opens the existing anime popup
   (full details, synopsis, score, play, add-to-list).

## Architecture

### New file

- `src/screens/related.rs` — `RelatedScreen` struct implementing the `Screen`
  trait.
- Registered in the `define_screens!` macro in `src/screens/mod.rs`:
  `RELATED => "Related" => related::RelatedScreen`.
- Added to the navbar via `NavBar::add_screen(screens::RELATED)` wherever the
  other screens are registered.
- Participates in screen caching via the `add_screen_caching!` macro, matching
  the other screens.

### Reused components

- `crate::utils::input::Input` — the search text box.
- Shared `anime_store` (keyed by `AnimeId`) from `ExtraInfo` — every fetched
  anime (root, chain nodes, branch nodes) is inserted here so the popup overlay
  can find it.
- `Action::ShowOverlay(AnimeId)` — emitted on node selection to open the shared
  anime popup (handled in `app.rs`).
- Background-thread pattern from `search.rs` / `popup.rs` — a worker thread with
  a `Sender<LocalEvent>` and results delivered back through the app event
  channel.
- `ImageManager` — optional small thumbnails on timeline nodes (may be deferred
  if it complicates the strip layout; not required for v1).

### New MAL client method

`MalClient::get_anime_by_id(&self, id: u64) -> Option<Anime>`

- Fetches a single anime with the full field set: `GET {BASE_URL}/anime/{id}?fields=…`.
- The single-anime endpoint returns one anime **object**, not the
  `{ data: [...], paging }` list shape that `AnimeResponse` expects. This needs a
  fetch/deserialize path that decodes `Anime` directly (a small addition next to
  the existing `send_request` / `fetch_anime` plumbing).
- Used to follow the relation chain: each `related_anime` entry only carries
  `{ id, title, main_picture }`, so to learn a node's own relations the node must
  be re-fetched by id.

## Screen modes

A `Mode` enum drives rendering and input:

- `Search` — typing a query.
- `Picking` — choosing from search results.
- `Timeline` — viewing the built timeline (with a "building" sub-state while the
  chain is still being fetched).

Focus interacts with the navbar following the existing pattern (`NavbarSelect`).

## Background work

A worker thread owns a `Receiver<LocalEvent>`; the screen holds the `Sender`.

- `LocalEvent::Search(query)`:
  - Calls `mal_client.search_anime(query, 0, limit)`.
  - Inserts results into the shared store; delivers the list of `AnimeId`s back
    to the screen (via the app channel / `BackgroundUpdate`).
  - Screen transitions to `Picking`.

- `LocalEvent::Build(root_id)`:
  - Builds the timeline (algorithm below).
  - Inserts every fetched main-line anime into the shared store.
  - Delivers the assembled `Timeline` structure back to the screen.
  - Screen transitions to `Timeline`.

- `LocalEvent::FetchBranch(id)`:
  - Just-in-time `get_anime_by_id(id)` for a selected branch node not yet in the
    store; inserts the full anime into the shared store and signals the screen so
    it can emit `Action::ShowOverlay(id)`.

### Timeline-building algorithm

```
visited: HashSet<u64>
CAP = 25 nodes

fetch root (already have it from search; ensure full fields)
mark root visited

# walk backward (prequels) -> prepend to line
node = root
loop:
    next = first relation of node where relation_type == "prequel"
    if none or next.id in visited or line.len() >= CAP: break
    fetch next by id; mark visited; prepend to line; node = next

# walk forward (sequels) -> append to line
node = root
loop:
    next = first relation of node where relation_type == "sequel"
    if none or next.id in visited or line.len() >= CAP: break
    fetch next by id; mark visited; append to line; node = next

# branches: for each node on the line, collect its non-prequel/sequel relations
for each line_node:
    branches[line_node.id] = [
        (relation_type_formatted, related)   # `related` is the related_anime Node
        for rel in line_node.related_anime
        if rel.relation_type not in {"prequel", "sequel"}
           OR rel is an extra prequel/sequel beyond the first (the chosen line)
    ]
    # branches are NOT fetched or recursed during build.
```

Notes:
- Multiple prequels or sequels on one node: the **first** continues the main
  line; the rest become branches on that node.
- `visited` prevents infinite loops from bidirectional/cyclic relations.
- `CAP` (25) bounds API calls and screen width; if hit, the UI shows a
  "…more not shown" marker.

### Fetch policy (which nodes get full data, when)

- **Main-line nodes** are fully fetched during build (required to follow the
  chain), so they are in the shared store with complete details. Selecting one
  emits `Action::ShowOverlay(id)` directly.
- **Branch nodes** are *not* fetched during build — only the `related_anime`
  Node data (`id`, `title`, `main_picture`) is kept, which is enough to display
  the branch label and title. This avoids paying for branches the user never
  opens. When the user selects a branch, the worker performs a just-in-time
  `get_anime_by_id(id)`, inserts the full anime into the shared store, and then
  the screen emits `Action::ShowOverlay(id)`. The detail panel therefore shows a
  score for main-line nodes and title-only for not-yet-opened branches.

## Data model

```rust
struct Timeline {
    root: u64,
    line: Vec<AnimeId>,                            // chronological: prequel..root..sequel (full anime in store)
    branches: HashMap<u64, Vec<BranchEntry>>,      // per line-node id -> its branch relations
}

struct BranchEntry {
    relation_label: String,  // relation_type_formatted, e.g. "Side story"
    id: u64,                 // related anime id (fetched just-in-time on select)
    title: String,           // from the related_anime Node (for display)
}
```

The screen also tracks: current `Mode`, the search `Input`, the picking result
list, the selected line index, the selected branch index (when focus is in the
branch panel), and a horizontal scroll offset for the strip.

## Layout (Timeline mode)

- **Top:** search bar showing the current query (re-searchable).
- **Middle:** horizontal, scrollable strip of node boxes joined by `──`
  connectors. The selected node is highlighted; `◀`/`▶` indicate off-screen
  nodes. Each box shows the (truncated) title and score.
- **Bottom:** a detail panel for the currently selected node — title, type,
  episodes, status, score — plus a list of that node's branch relations, each
  selectable.

**Picking mode** reuses the top search bar and shows a simple selectable list of
matches (title + year/type/episodes).

### Input

- `←` / `→` — move selection along the main line (scrolls the strip).
- `↑` / `↓` — move focus into / out of the branch list of the selected node.
- `Enter` — open the popup for the selected node or branch entry
  (`Action::ShowOverlay(id)`).
- Search bar focus follows the existing input + navbar conventions.

## Error handling & edge cases

- **No search results:** show an inline message; stay in Search/Picking.
- **Anime with no relations:** timeline is a single node (just the root).
- **Cycles / bidirectional links:** guarded by the `visited` set.
- **Cap reached:** show a "…more not shown" marker at the truncated end.
- **Fetch failure mid-build:** keep the partial timeline already assembled and
  surface a soft error (e.g. via `Action::ShowError` or an inline notice) rather
  than discarding everything.
- **Building state:** while the chain is still fetching, show a "Building…"
  indicator; the strip can fill in progressively or appear once complete
  (progressive is preferred if cheap, otherwise deliver once complete).

## Testing

- Unit-test the timeline-building algorithm with synthetic `Anime` graphs:
  - linear prequel/sequel chain → correct ordered `line`.
  - branching (multiple sequels) → first on line, extras in `branches`.
  - cyclic relations → terminates, no duplicates.
  - cap enforcement → stops at `CAP`, marks truncation.
  - non-sequel/prequel relations → land in `branches`, not the line.
- Manual/integration: run the app, search a multi-season franchise (e.g. Attack
  on Titan), verify the ordered line, branch listing, scrolling, and that
  selecting nodes opens the popup with correct details.
