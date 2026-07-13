#!/usr/bin/env bash
# Build a small, fully-linked spec->impl map so the agent's job is purely
# *understanding* it: cover both repos, split each into a preamble + one
# named segment per endpoint / handler, connect spec->impl, link each
# endpoint to its handler, and label the two auth-bearing endpoints so a
# cross-cutting "auth" concern exists for the label lens.
#
# T9 is about the inspection access pattern (bounded drill-down + label
# lens), not building or repair: the map is already complete and clean.
set -euo pipefail

# --- spec: preamble + 4 endpoints (headings at 5, 11, 18, 25) -----------
( cd spec && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd spec && "$COVMAP_BIN" split main:api.md:1   5  preamble get-widgets    ) >/dev/null
( cd spec && "$COVMAP_BIN" split main:api.md:5  11  get-widgets post-widgets ) >/dev/null
( cd spec && "$COVMAP_BIN" split main:api.md:11 18  post-widgets del-widget  ) >/dev/null
( cd spec && "$COVMAP_BIN" split main:api.md:18 25  del-widget healthz       ) >/dev/null
( cd spec && "$COVMAP_BIN" name main:api.md:5  spec-get     ) >/dev/null
( cd spec && "$COVMAP_BIN" name main:api.md:11 spec-post    ) >/dev/null
( cd spec && "$COVMAP_BIN" name main:api.md:18 spec-delete  ) >/dev/null
( cd spec && "$COVMAP_BIN" name main:api.md:25 spec-health  ) >/dev/null
# The two endpoints that require authentication carry the `auth` label —
# a concern that is spread across the file, not one contiguous region.
( cd spec && "$COVMAP_BIN" label main:@spec-post   auth ) >/dev/null
( cd spec && "$COVMAP_BIN" label main:@spec-delete auth ) >/dev/null

# --- impl: preamble + 4 handlers (fns at 3, 8, 19, 30) ------------------
( cd impl_rs && "$COVMAP_BIN" init && "$COVMAP_BIN" cover ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" split main:handlers.rs:1   3  preamble list-fn     ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" split main:handlers.rs:3   8  list-fn create-fn    ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" split main:handlers.rs:8  19  create-fn delete-fn  ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" split main:handlers.rs:19 30  delete-fn health-fn  ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" name main:handlers.rs:3  impl-list   ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" name main:handlers.rs:8  impl-create ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" name main:handlers.rs:19 impl-delete ) >/dev/null
( cd impl_rs && "$COVMAP_BIN" name main:handlers.rs:30 impl-health ) >/dev/null

# --- connect + link each endpoint to its handler -----------------------
( cd spec && "$COVMAP_BIN" connect main "../impl_rs:main" apimap ) >/dev/null
( cd spec && "$COVMAP_BIN" link apimap @spec-get    @impl-list   "GET list"        ) >/dev/null
( cd spec && "$COVMAP_BIN" link apimap @spec-post   @impl-create "POST create"     ) >/dev/null
( cd spec && "$COVMAP_BIN" link apimap @spec-delete @impl-delete "DELETE by id"    ) >/dev/null
( cd spec && "$COVMAP_BIN" link apimap @spec-health @impl-health "liveness probe"  ) >/dev/null

echo "Setup done. spec/main -> impl_rs/main via 'apimap' (4 links, all named)."
echo "Two endpoints are labelled 'auth'. The map is complete and drift-free."
