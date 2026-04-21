# GAPShift Pairwise Experiment

This document describes the current experimental pairwise mode. It is separate
from the older GAPShift metadata-host design.

## Purpose

The pairwise experiment asks a simpler question:

Can blocks be transformed pair by pair without metadata, while still allowing
the original numeric set to be recovered?

The answer is yes for the sorted value set.

The answer is no for original record order, unless index metadata is added.

## Current Defaults

```text
BIT_WIDTH = 4096
COUNT     = 1000
```

Each value is a 4096-bit unsigned integer written as one binary line.

## Pairwise Encode

Command:

```bash
program -p input.txt pairwise_encoded.txt
```

The encoder:

1. Reads all blocks.
2. Sorts the values.
3. Splits them into adjacent pairs:

```text
(block 1, block 2)
(block 3, block 4)
(block 5, block 6)
...
```

4. For each pair:

```text
small < large
raw_gap = large - small
gap     = raw_gap - 2
```

5. Writes the pair as:

```text
gap
large
```

So the encoded file is:

```text
gap_1
large_1
gap_2
large_2
gap_3
large_3
...
```

## Pairwise Decode

Command:

```bash
program -pd pairwise_encoded.txt pairwise_decoded.txt
```

For every two encoded lines:

```text
gap
large
```

the decoder reconstructs:

```text
small = large - (gap + 2)
large = large
```

The decoded output is sorted before writing.

## What It Can Do

- It can transform each adjacent sorted pair without external metadata.
- It can recover the exact sorted value set.
- It avoids the global rank-shift underflow problem.
- It keeps total bit entropy almost unchanged in the current tests.
- It exposes leading-bit changes clearly.

Observed on the current 1000-block input:

```text
Pairwise input entropy   : 1.000000 bits/bit
Pairwise encoded entropy : 0.999999 bits/bit
Entropy delta            : -0.000001 bits/bit
```

Leading-bit observation:

```text
Input   first-bit 1s=497, 0s=503, avg leading run=1.998
Encoded first-bit 1s=249, 0s=751, avg leading run=6.266
```

This means total entropy barely moves, but the beginning of the blocks changes
substantially.

## What It Cannot Do

- It cannot restore original record order.
- It cannot decode correctly if pair order is shuffled.
- It cannot preserve per-line identity without additional index metadata.
- It is not encryption.
- It is not standard compression.
- It is a sorted-set reversible transform, not an ordered-record reversible
  transform.

## Why Metadata Is Not Needed Here

For a pair:

```text
small < large
gap = large - small - 2
```

the encoded pair keeps:

```text
gap
large
```

Decode can recover:

```text
small = large - gap - 2
```

No host index, companion index, or GAP overflow metadata is needed for this
pairwise model.

## Reports

By default, commands print the short comparison:

```text
Entropy change
Leading bit change
```

Use `-reports` for detailed reports:

```bash
program -p input.txt pairwise_encoded.txt -reports
program -pd pairwise_encoded.txt pairwise_decoded.txt -reports
```

Detailed reports include:

- full bit entropy,
- one/zero counts,
- first-bit distribution,
- average leading-run length,
- proximity between sorted values,
- distance to zero,
- distance to the top of the 4096-bit range.

## Comparison With Global-Shift GAPShift

The older global-shift model uses:

```text
shift(rank) = (rank + 1) * GAP
encoded     = value - shift(rank)
```

That model can preserve original order when metadata is present, but it can also
fail when the selected GAP makes the rank shift larger than early small values.

The pairwise model avoids that class of underflow because it does not subtract a
global rank shift from every block. It only stores:

```text
gap = large - small - 2
```

and keeps `large` unchanged.

## Status

This is an experimental mode. It is useful for studying:

- sorted-set reversibility,
- entropy movement,
- leading-bit changes,

## Author

Mesut Erturhan
April 20, 2026 - Update 22-04-2026

---

## License

GPL-3.0 (GNU General Public License v3.0)
- metadata-free pair transforms.



It is not yet a replacement for an ordered reversible block transform.

