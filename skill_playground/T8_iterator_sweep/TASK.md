# T8 — Sweep a covering as a worklist with an iterator

## User prompt (hand this to the agent)

> covmap is initialized in `spec/` and `spec/api.md` is split into five
> segments: a preamble and one named segment per HTTP endpoint
> (`@get-widgets`, `@post-widgets`, `@delete-widget`, `@healthz`).
>
> I need a security review of every endpoint. Please walk the covering
> one segment at a time and record a one-line verdict for each — whether
> the endpoint's spec says anything about authentication or access
> control. Do it in a way that survives you losing your train of thought
> partway through, and make sure no segment is missed. Tell me the final
> tally when you're done.

## Starting state

- `spec/.covmap/` is populated.
- `spec/main` has 5 segments: an anonymous `preamble` and four named
  endpoint segments (`@get-widgets`, `@post-widgets`, `@delete-widget`,
  `@healthz`).
- No connection, no drift. This task is about *enumeration*, not repair.

## Expected agent behavior

1. **Freeze the covering into an iterator** so the plan lives on disk and
   is resumable:

   ```sh
   covmap iter review new main --order file
   ```

2. **Run the canonical loop**, pulling one task at a time and resolving it
   before advancing:

   ```sh
   until covmap iter review finished; do
     covmap iter review next               # first pending task + its frozen bytes
     covmap iter review done -m "<verdict>"  # or: skip -m "<why>"
   done
   ```

   `next` is refused while the current task is still pending, and it always
   advances to the first pending task in order — so no endpoint is stepped
   over and every one gets a verdict.

3. **Record a verdict per task** in the note (`-m`). It is fine to `skip`
   the anonymous preamble (it is not an endpoint) with a reason. For at least
   one endpoint, use a second `-m` paragraph for the evidence you checked.

4. **Report the tally** with `covmap iter review status`
   (e.g. `5/5 resolved (4 done, 1 skipped) — finished`).

The resume affordance is `covmap iter review current`: after any interruption
it reprints exactly the task the cursor is on together with its frozen bytes.
Use `--no-content` only when a script needs the compact task row.

## Why this matters

Agents drift and lose the thread on large, repetitive jobs. An iterator
turns a covering into a durable cursor plus a single in-scope chunk: each
step is small, verifiable, and resumable, because the plan is on disk, not
in a context window. The enumeration guarantee (advance only off a
resolved task; `finished` only when nothing is pending) is what makes
"review every segment exactly once" a property the tool enforces rather
than a discipline you have to maintain by hand.

## Success criteria (graded)

The grader checks that:

- an iterator over `main` exists with one task per segment (5 tasks);
- **every task is resolved** — `finished` is true and no task is still
  pending (complete enumeration);
- each of the four endpoint tasks carries a non-empty note (a verdict);
- at least one endpoint verdict has two paragraphs;
- the covering was **not mutated** by the sweep (an iterator is a snapshot
  layered over the covering — it never edits segments).

## Grading

```bash
./grade.sh
```
