# T9 — Understand a mapping with bounded drill-down and the label lens

## User prompt (hand this to the agent)

> covmap is set up in `spec/` with a connection `apimap` mapping the HTTP
> API spec (`spec/api.md`) to its Rust handlers (`impl_rs/handlers.rs`).
> Every endpoint is linked to its handler.
>
> Two questions:
> 1. How does the **DELETE** endpoint map to the implementation — what
>    handler realizes it, and where is it?
> 2. Authentication is a cross-cutting concern. Which parts of the spec
>    involve authentication, and where are they implemented?
>
> Answer from covmap's output — don't re-read the whole files by hand.

## Starting state

- `spec/.covmap/` and `impl_rs/.covmap/` are populated.
- `spec/main` has 5 segments: `preamble` + four named endpoints
  (`@spec-get`, `@spec-post`, `@spec-delete`, `@spec-health`). The two
  authenticated endpoints (`@spec-post`, `@spec-delete`) carry the label
  `auth`.
- `impl_rs/main` has 5 segments: `preamble` + four named handlers
  (`@impl-list`, `@impl-create`, `@impl-delete`, `@impl-health`).
- Connection `apimap` links each endpoint to its handler (4 links, all by
  `@name`). The map is complete and drift-free.

## Expected agent behavior

The point is to reach understanding in a *few bounded calls*, ending on a
view that resolves the relation — not to dump whole coverings and grep.

1. **Orient** (optional): `covmap status` shows `apimap` with its coverage.

2. **Understand the DELETE endpoint** — a single-segment drill-down:

   ```sh
   covmap show main:@spec-delete
   ```

   This prints the segment plus its resolved outgoing link — the handler
   (`@impl-delete`, `handlers.rs:19..29`) it maps to — in one call. A
   region view (`covmap show main:api.md:18..24`) works too.

3. **Trace the auth concern** — the label lens, one call:

   ```sh
   covmap show --label auth main
   ```

   This lists both `auth`-labelled endpoints (`@spec-post`, `@spec-delete`)
   each with the handler it maps to, so the whole concern and its
   implementation come back together.

4. **Report** the DELETE mapping and the two authenticated endpoints with
   their handlers, citing locations from the output.

## What this exercises

- **Bounded drill-down** (req23): understanding a region in ≤3 calls, the
  last one self-sufficient (endpoints resolved to `label loc`).
- **The label lens**: pulling a cross-cutting concern spread across a file
  into a single command via a shared label.
- Not building, not repair: the map is already correct.
