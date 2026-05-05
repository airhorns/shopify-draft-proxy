---
name: linear-issue-cache
description: |
  Build and query a local qmd-indexed cache of Linear issues. Use when an
  automated workflow needs to spam Linear issue search (duplicate detection,
  cross-project audits, batch issue generation) without exhausting Linear's
  API rate limits, or when prior agents already populated `.linear-cache/`.
---

# Linear Issue Cache (qmd)

Linear's API rate limit (~1500 req/hr per personal key) is hostile to
automated workflows that need to look across hundreds of issues — for
example duplicate-detection passes inside the
`shopify-area-issue-generation` skill, or any loop that grep-walks every
issue in a project. Instead, sync once into a local qmd index and search
that.

The cache is a directory of one-markdown-file-per-issue under
`.linear-cache/issues/`, indexed by [qmd](https://github.com/tobi/qmd) for
hybrid BM25 + vector + LLM-rerank search. The cache directory is
gitignored.

## When to use this skill

- An automated workflow needs to search Linear issues at high volume.
- You want semantic/full-text search across issue titles and bodies, not
  just GraphQL filter expressions.
- You are doing duplicate detection before creating new issues.
- You need to grep issue contents while offline.

## When **not** to use this skill

- You need a single specific issue by key — use the `linear` skill
  (`linear_graphql`) or the `mcp__linear-server__get_issue` tool. Cached
  data may be stale.
- You need to mutate Linear — caching is read-only.
- You need state-of-the-world freshness (assignee, status as of right
  now). Refresh first or query Linear directly.

## Prerequisites

1. **qmd CLI** on `PATH`. Install with:
   ```bash
   bun install -g @tobilu/qmd
   # or, if mise is available:
   # mise use -g npm:@tobilu/qmd
   ```
   Verify with `qmd --version`.

2. **`LINEAR_API_KEY`** in the environment (or a `.env` file at repo
   root). Create a personal API key at
   <https://linear.app/settings/api>. The script reads `.env` via
   `dotenv/config`, so adding `LINEAR_API_KEY=lin_api_...` to `.env` is
   enough.

## Building / refreshing the cache

From the repo root:

```bash
# First sync (fetches every accessible issue, then indexes + embeds):
pnpm linear:cache

# Subsequent runs are incremental — only issues with updatedAt > the
# previously seen cursor are fetched and rewritten:
pnpm linear:cache

# Force a full re-fetch (wipes the updatedAt cursor):
pnpm linear:cache --full

# Restrict to one or more team keys (e.g. SHOP and CORE):
pnpm linear:cache --team SHOP --team CORE

# Skip qmd indexing (just write markdown files):
pnpm linear:cache --no-index
```

The script:

1. Pages through `issues(...)` via Linear's GraphQL API
   (`includeArchived: true`, ordered by `updatedAt`), inlining the first
   50 comments per issue and paginating any overflow.
2. Writes each issue to
   `.linear-cache/issues/<IDENTIFIER>.md` with YAML frontmatter
   (identifier, title, url, state, team, project, cycle, priority,
   estimate, assignee, creator, parent, branch_name, labels,
   comment_count, last_comment_at, timestamps) followed by the
   description and a `## Comments` section in chronological order.
3. Persists the highest `updatedAt` it saw to
   `.linear-cache/state.json` for incremental resumes.
4. Registers the `linear-issues` qmd collection if missing, then runs
   `qmd update` and `qmd embed`.

## Searching the cache

All search uses the `linear-issues` collection:

```bash
# Hybrid: auto-expanded BM25 + vector + rerank (best quality, slower)
qmd query -c linear-issues "duplicate detection for customer area issues"

# Pure BM25 keywords (fastest — good for spam-volume search)
qmd search -c linear-issues "metafield definition pin limit"

# Vector-only semantic similarity
qmd vsearch -c linear-issues "issues about address validation"

# Structured query document (mix lex + vec + hyde)
qmd query -c linear-issues "$(printf 'lex: customer merge\nvec: how does customer merge attach resources')"

# JSON output with score traces — useful for programmatic dedupe
qmd query -c linear-issues --json --explain "your question"

# Retrieve a specific cached issue verbatim
qmd get issues/SHOP-1234.md
```

Useful flags:

- `-c, --collection linear-issues` — restrict to this cache.
- `-l N` — limit results.
- `--json` — machine-readable output (use this in scripts).
- `--explain` — include score breakdowns for debugging ranking.

For workflows that already run inside this repo, calling `qmd` via
`execFileSync` from a tsx script is the recommended path; do not import
qmd as a library.

## Cache layout

```
.linear-cache/
├── issues/
│   ├── SHOP-1234.md
│   ├── SHOP-1235.md
│   └── …
└── state.json        # { "lastSyncedAt": "2026-05-05T12:34:56.000Z" }
```

Each issue file looks like:

```markdown
---
identifier: "SHOP-1234"
title: "Add metafield definition pin limit conformance"
url: "https://linear.app/.../SHOP-1234"
state: "Backlog"
state_type: "backlog"
team: "SHOP"
team_name: "Shopify"
project: "Conformance expansion"
priority: 3
comment_count: 4
last_comment_at: "2026-03-29T..."
created_at: "2026-01-04T..."
updated_at: "2026-04-02T..."
labels: ["conformance", "metafields"]
---

# SHOP-1234: Add metafield definition pin limit conformance

<issue description here as Markdown>

## Comments

### Alice · 2026-01-05T14:22:00.000Z

First comment body…

### Bob · 2026-01-06T09:13:00.000Z (edited 2026-01-06T09:30:00.000Z)

Reply body…
```

The qmd index itself lives in `~/.cache/qmd/index.sqlite` (shared across
projects).

## Operating notes

- **Freshness.** The cache is only as fresh as the most recent
  `pnpm linear:cache` run. Before acting on cached results in a
  high-stakes workflow (e.g. about to create issues), refresh
  incrementally; that costs only a small number of GraphQL pages.
- **Comments are cached.** All comments for each issue are appended
  under a `## Comments` section in chronological order, with author and
  `createdAt` (and `editedAt` when present). Issue `updatedAt` is bumped
  by Linear when comments are added/edited/deleted, so incremental
  syncs pick up comment changes automatically. Threading is flattened —
  if you need exact reply-tree structure or unredacted edit history,
  fall back to `linear_graphql`.
- **No deletion handling.** Issues deleted in Linear are not removed
  from the cache. Periodic `--full` runs after `rm -rf .linear-cache`
  clean this up if it matters.
- **Rate limit budget.** A full sync of a workspace with N issues takes
  `ceil(N/100)` GraphQL requests. Incremental syncs are usually a
  handful. Both are far cheaper than per-issue searches against Linear.
- **Concurrency.** qmd uses a single SQLite index; do not run
  `pnpm linear:cache` twice in parallel.
