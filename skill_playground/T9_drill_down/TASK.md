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
> 3. Review `api.md` and `errors.md` together. Which implementation segment is
>    a fan-in hub, and what does the error-contract mapping note say?
>
> Answer from covmap's output — don't re-read the whole files by hand.

## Starting state

- `spec/.covmap/` and `impl_rs/.covmap/` are populated.
- `spec/main` has 6 segments: `preamble` + four named endpoints
  (`@spec-get`, `@spec-post`, `@spec-delete`, `@spec-health`). The two
  authenticated endpoints (`@spec-post`, `@spec-delete`) carry the label
  `auth`. `errors.md` is the named `@auth-errors` segment.
- `impl_rs/main` has 5 segments: `preamble` + four named handlers
  (`@impl-list`, `@impl-create`, `@impl-delete`, `@impl-health`).
- Connection `apimap` has 5 named links. `@spec-post` and `@auth-errors` both
  map to `@impl-create`, forming a fan-in hub. The error-contract link has a
  long rationale note. The map is complete and drift-free.

## Expected agent behavior

The point is to reach understanding in a *few bounded calls*, ending on a
view that resolves the relation — not to dump whole coverings and grep.

1. **Orient** (optional): `covmap status` shows `apimap` with its coverage.

2. **Narrow the review to both relevant files** — one multi-region listing:

   ```sh
   covmap ls -l main api.md errors.md
   ```

   The union is returned in covering order, with each segment printed once.

3. **Understand the DELETE endpoint** — a single-segment drill-down:

   ```sh
   covmap show main:@spec-delete
   ```

   This prints the segment plus its resolved outgoing link — the handler
   (`@impl-delete`, `handlers.rs:19..29`) it maps to — and the segment's
   recorded content in one call. A region view (`covmap show
   main:api.md:18..24`) shows every intersecting source segment in full with
   its content and mapping. Add `--no-content` for relationships only.

4. **Trace the auth concern** — the label lens, one call:

   ```sh
   covmap show --label auth main
   ```

   This lists both `auth`-labelled endpoints (`@spec-post`, `@spec-delete`)
   each with the handler it maps to, so the whole concern and its
   implementation come back together.

5. **Review the connection** — `covmap show apimap` reports max-in 2 and the
   create handler as the fan-in hub. Its link list places the long
   error-contract note on wrapped lines below the resolved pair.

6. **Report** the DELETE mapping, the two authenticated endpoints with their
   handlers, and the error-contract hub/note, citing locations from the output.

## What this exercises

- **Bounded drill-down** (req23): understanding a region in ≤3 calls, the
  last one self-sufficient (endpoints resolved to `label loc`).
- Multi-region listing, resolved degree hubs, and readable mapping notes.
- **The label lens**: pulling a cross-cutting concern spread across a file
  into a single command via a shared label.
- Not building, not repair: the map is already correct.
