# GAPShift

GAPShift is a reversible transformation for 256-bit integers.

In plain terms, it takes a collection of large numbers, shifts them in a controlled way, and later reconstructs the original values exactly. The method is deterministic, compact, and designed to keep metadata overhead low by embedding most of the recovery information directly into the encoded data.

This project is not an encryption system and it is not a classical compression algorithm. Its purpose is to reorganize data inside the numeric space while preserving exact recoverability.

---

## Why this project exists

If we simply subtract arbitrary values from a dataset, recovery quickly becomes messy. We would normally have to store a lot of extra information to undo the transformation later.

GAPShift takes a different route:

- it looks at the internal spacing of the data,
- derives one global step value from that structure,
- applies a simple linear rule,
- and keeps the information needed for recovery as small as possible.

The interesting part is that the transformation remains fully reversible even though the metadata footprint is kept small.

---

## High-level idea

The workflow is built around one value called **GAP**.

First, the numbers are sorted.  
Then the algorithm finds the **closest adjacent pair** in that sorted order.  
The distance between those two numbers is the raw local spacing:

```text
raw_gap = sorted[i+1] - sorted[i]
```

From that value, the operational step is defined as:

```text
GAP = raw_gap - 2
```

That single GAP value is then used across the whole sorted dataset.

For a value at sorted rank `r`, the shift is:

```text
shift(r) = (r + 1) * GAP
```

and the encoded value becomes:

```text
y = x - shift(r)
```

So the first sorted value is shifted by `1 * GAP`, the second by `2 * GAP`, the third by `3 * GAP`, and so on.

This makes the system easy to reason about and easy to reverse.

---

## Why `raw_gap - 2`?

This small adjustment is what gives the system its recovery anchor.

Suppose the closest adjacent pair is:

```text
15, 20
```

Then:

```text
raw_gap = 20 - 15 = 5
GAP = 5 - 2 = 3
```

After encoding with consecutive shifts, the encoded distance of that pair becomes exactly `2`.

That matters because one of those two blocks will later be reused as a metadata host. During decoding, the original encoded value of that overwritten block can be reconstructed with a very simple rule:

```text
old_low_output = high_output - 2
```

This is one of the key ideas in the design.

---

## Metadata design

The project tries to avoid storing large external metadata.

Instead of keeping GAP and block references in separate files, one selected encoded block becomes a **metadata host block**.

That block stores:

- the recovered `GAP`,
- the original position of its companion block.

These two values are packed into the same 256-bit block.

Only one small external hint remains:

### `metadata.txt`

This file stores only the original position of the metadata host block.

Its width is not fixed. It is calculated dynamically from the total number of records:

```text
position_bits = ceil(log2(COUNT))
```

That means the metadata field grows only as much as needed.

Examples:

- `COUNT = 10,000` → `14` bits
- `COUNT = 5,000,000` → `23` bits

This makes the addressing part of the metadata scalable without wasting space.

---

## Important detail about block order

The transformation itself is defined on **sorted ranks**, but the exported files keep the **original block order**.

That means:

- the algorithm sorts values internally to choose GAP and apply the rank-based shift,
- but `input.txt` and `encode.txt` are written in the original record order.

This is intentional. The system works with real block positions, not a rewritten sorted file layout.

---

## Encode flow

The encoding stage can be summarized as follows:

1. Generate or read the input values.
2. Sort them in ascending order.
3. Find the closest adjacent pair.
4. Compute `GAP = raw_gap - 2`.
5. Shift every sorted value according to its rank.
6. Choose one block from the selected pair as the metadata host.
7. Pack `GAP` and the companion block position into that host block.
8. Write the final encoded data back in original block order.

---

## Decode flow

The decoder does not need the original input file to reconstruct the rank structure.

It works from `encode.txt` and `metadata.txt` like this:

1. Read the metadata host position from `metadata.txt`.
2. Read the metadata host block from `encode.txt`.
3. Extract:
   - `GAP`
   - companion block position
4. Rebuild the overwritten source-pair value using:

```text
old_low_output = high_output - 2
```

5. Once the selected pair is restored, sort the encoded values **in memory**.
6. Reconstruct the original numbers using:

```text
x = y + shift(rank)
```

7. Write the decoded values back in original block order.

This keeps the external decode requirements small while still allowing the original order structure to be recovered.

---

## A small example

Take this sorted sequence:

```text
15
20
39
```

The closest adjacent pair is `15` and `20`.

So:

```text
raw_gap = 5
GAP = 3
```

Now apply the linear rule:

```text
15 -> 12
20 -> 14
39 -> 30
```

The encoded difference of the selected pair is now:

```text
14 - 12 = 2
```

If the lower block of that pair is later replaced with packed metadata, the decoder can still restore its old encoded value using the higher encoded output:

```text
old_low_output = 14 - 2 = 12
```

Then normal decode can continue.

This example is small, but the same logic works for large 256-bit datasets.

---

## Files produced by the program

### `input.txt`
Original values as plain 256-bit binary strings, one value per line.

### `encode.txt`
Encoded values as plain 256-bit binary strings, one value per line, written in original block order.

### `metadata.txt`
The original position of the metadata host block. The width is dynamic and depends on the number of blocks.

### `gap.txt`
Technical information about the selected source pair, `raw_gap`, `GAP`, and related encoded values.

### `space.txt`
A detailed debug-oriented record of the transformation, including roles, shifts, input values, encoded values, and decoded values.

### `smallest_two_outputs.txt`
The two smallest encoded outputs, written for quick inspection.

---

## Decode mode

You can decode with:

```bash
program -d <source_bits_file> <decoded_output_file>
```

Example:

```bash
program -d encode.txt decoded.txt
```

The decoder will:

- find the metadata host,
- recover `GAP` and the companion location,
- restore the overwritten pair,
- sort the encoded values in memory,
- and reconstruct the original dataset.

---

## What this project is and is not

GAPShift is:

- deterministic,
- reversible,
- structured,
- metadata-aware,
- suitable for experiments in reversible data-space transformation.

GAPShift is not:

- a cipher,
- a hash function,
- a security primitive,
- or a standard entropy-compression method.

It moves data in a disciplined way. It does not try to hide the existence of structure.

---

## Design goals

This implementation tries to balance three things:

1. **reversibility** — the original data must be recovered exactly,
2. **simplicity** — the core rule should stay understandable,
3. **low metadata cost** — recovery data should be kept as small and local as possible.

The current metadata-host design is the practical result of those goals.

---

## Limitations

A few boundaries are worth stating clearly:

- The method depends on a valid closest-pair source.
- The chosen GAP must satisfy underflow safety for every sorted rank.
- The metadata host block must remain readable and unchanged.
- This design is structural, not cryptographic.
- Very large stress tests may require careful review of metadata packing width, memory use, and file size.

These are normal engineering limits, not hidden weaknesses.

---

## Project status

This repository is experimental and evolving. The core reversible logic is already in place, but metadata strategies, stress-test behavior, and packing schemes are still being refined.

That is part of the purpose of the project: to explore how far a compact reversible transformation can go while staying simple enough to reason about.

---

## Author

Mesut Erturhan

April 20, 2026  
Updated April 21, 2026

---

## License

GPL-3.0 (GNU General Public License v3.0)
