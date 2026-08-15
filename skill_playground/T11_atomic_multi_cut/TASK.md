# T11 - Refine a file with one atomic multi-cut

## User prompt (hand this to the agent)

> covmap is initialized and `spec/service.md` is still one whole-file
> segment. Refine it into a preamble plus one segment for each `##` section.
> Insert all section boundaries atomically in one `split` command, even though
> I am giving you the heading lines out of order: 18, 5, 14, 9. Then label the
> resulting segments `preamble`, `Parse`, `Validate`, `Execute`, and `Report`.
> Finish by reporting status and integrity.

## Starting state

- `spec/.covmap/` exists with format version 2.
- Covering `main` has one `unknown` whole-file segment for `service.md`.
- The `##` headings begin at lines 5, 9, 14, and 18.

## Expected agent behavior

Run every covmap command below from `spec/`.

1. Inspect the covering with `covmap ls -l`.
2. Insert every boundary in one operation:

   ```sh
   covmap split main:service.md 18 5 14 9
   ```

   `split` validates and sorts the complete cut set before replacing the
   covering ref once. It creates five `unknown` fragments.
3. Apply status metadata separately:

   ```sh
   covmap label main:service.md:1  preamble
   covmap label main:service.md:5  Parse
   covmap label main:service.md:9  Validate
   covmap label main:service.md:14 Execute
   covmap label main:service.md:18 Report
   ```

4. Run `covmap status` and `covmap fsck`.

Do not add a `segment`/`unsegment` command, edit `.covmap/` refs manually, or
pass labels as `split` operands.

## Success criteria (graded)

- `main` contains exactly the ranges `1..4`, `5..8`, `9..13`, `14..17`,
  and `18..20`, in file order.
- Those ranges carry the five requested labels.
- Concatenating the stored segment objects reconstructs `service.md`
  byte-for-byte.
- The repository remains format v2 and `covmap fsck` passes.

## Grading

```bash
./grade.sh
```
