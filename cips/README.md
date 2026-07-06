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
governed by the five-artifact consistency contract in
[`../DEVELOPER.md`](../DEVELOPER.md): a change is not done until it is reconciled
across theory, requirements, implementation, skill, and tutorial. Each cip
should end with a **landing checklist** naming that fallout, so the gap between
"design accepted" and "feature shipped" is explicit.

Rough stages a proposal moves through:

1. **Draft** — under active design; the API and model are still changing.
2. **Accepted** — the design is settled and ready to implement.
3. **Landed** — the feature has been built and reconciled across all five
   artifacts; the cip stays as the historical design record.

## Naming

Proposals are numbered in creation order and titled with a short kebab-case
slug: `NNNN-<slug>.md` (e.g. `0001-iterators.md`). Numbers are never reused. The
number is an identifier, not a priority.

## Index

| # | Proposal | Stage | Summary |
| - | -------- | ----- | ------- |
| [0001](0001-iterators.md) | iterators | Landed | Freeze a covering into an ordered, stateful worklist — a durable, resumable work plan an agent or human pulls one task at a time. |

## Writing a cip

Aim for clear and concise; a cip is a spec, not an essay. A good one covers:

- **Idea** — what the feature is, in a paragraph.
- **Motivation** — the problem it solves; who it helps.
- **Model** — the data and, where relevant, the on-disk format.
- **Commands / API** — the exact surface, with signatures.
- **Invariants** — what must always hold.
- **Use cases** — concrete workflows that exercise the API.
- **Landing checklist** — the five-artifact reconciliation the feature will
  require (see [`../DEVELOPER.md`](../DEVELOPER.md)).
