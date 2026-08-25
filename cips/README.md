# cips — covmap improvement proposals

A **cip** (covmap improvement proposal) is a design document for a substantial
change to covmap: a new capability, a reworked command surface, or a shift in
the on-disk format. It captures *what* we want to build and *why*, and it is
refined until it reads as a spec an implementer can follow — before any code is
written.

Small bug fixes and mechanical changes do not need a cip; they go straight to a
commit. Reach for a cip when a change is large enough that the design deserves
its own review, or touches the model rather than just the implementation.

## Lifecycle

A cip is **not** an implemented feature. It is a design under discussion or
accepted-but-not-yet-built. Landing the feature it describes is a separate step
governed by the six-artifact consistency contract in
[`../DEVELOPER.md`](../DEVELOPER.md): a change is not done until it is reconciled
across theory, requirements, implementation, skill, tutorial, and glossary.
Each cip should end with a **landing checklist** naming that fallout, so the
gap between "design accepted" and "feature shipped" is explicit.

Rough stages a proposal moves through:

1. **Draft** — under active design; the API and model are still changing.
2. **Accepted** — the design is settled and ready to implement.
3. **Landed** — the feature has been built and reconciled across all six
   artifacts; the cip stays as the historical design record.

Keep both the proposal and its index entry at **Accepted** throughout
implementation and review. Change them to **Landed** only as part of the final
landing commit, after the implementation and all six artifacts are complete.

## Naming

Proposals are numbered in creation order and titled with a short kebab-case
slug: `NNNN-<slug>.md` (e.g. `0001-iterators.md`). Numbers are never reused. The
number is an identifier, not a priority.

## Index

| # | Proposal | Stage | Summary |
| - | -------- | ----- | ------- |
| [0001](0001-iterators.md) | iterators | Landed | Freeze a covering into an ordered, stateful worklist — a durable, resumable work plan an agent or human pulls one task at a time. |
| [0002](0002-segment-messages.md) | segment-messages | Landed | Attach an optional, non-hashed, possibly multi-line message to a segment, recording why it was cut / what it represents / why it is (not) mapped. |
| [0003](0003-bounded-drill-down.md) | bounded-drill-down | Landed | Reorganize inspection so understanding one region of the mapping takes ≤3 self-sufficient calls, with condensed-list vs full-detail modes and a one-call label lens. Output/query only, no format change. |
| [0004](0004-content-only-hash.md) | content-only-hash | Landed | Drop the label from the segment hash so identity is a pure function of content; label joins name/message as ref-local metadata. Reverses req4; on-disk format change (v1→v2) with a `covmap migrate`. |
| [0006](0006-mapping-review-surfaces.md) | mapping-review-surfaces | Landed | Support multi-region listings and improve existing connection output with degree hubs and wrapped notes. |
| [0007](0007-connection-lifecycle-workflows.md) | connection-lifecycle-workflows | Landed | Complete connection deletion, formalize self-map syntax, align endpoint help, and improve iterator help. |
| [0009](0009-atomic-multi-cut.md) | atomic-multi-cut | Landed | Keep complete coverings and generalize `split` to label-free atomic cut-set insertion with one ref write. |
| [0010](0010-git-tracked-artifacts.md) | git-tracked-artifacts | Landed | Track portable covering and connection refs in Git while keeping reconstructible objects, peer paths, and work state local. |
| [0011](0011-covering-input-selection.md) | covering-input-selection | Landed | Add portable positive selection with global defaults and covering-specific policy overrides. |
| [0013](0013-uniform-multiline-messages.md) | uniform-multiline-messages | Landed | Give every message-bearing command repeatable paragraph input and add multiline-safe link/task note storage with one-line plumbing. |
| [0014](0014-content-first-inspection.md) | content-first-inspection | Accepted | Make focused segment, spatial file/range, link, and iterator-cursor views content-first; keep open-ended views compact and add `--no-content`. |

## Writing a cip

Aim for clear and concise; a cip is a spec, not an essay. A good one covers:

- **Idea** — what the feature is, in a paragraph.
- **Motivation** — the problem it solves; who it helps.
- **Model** — the data and, where relevant, the on-disk format.
- **Commands / API** — the exact surface, with signatures.
- **Invariants** — what must always hold.
- **Use cases** — concrete workflows that exercise the API.
- **Landing checklist** — the six-artifact reconciliation the feature will
  require (see [`../DEVELOPER.md`](../DEVELOPER.md)).
