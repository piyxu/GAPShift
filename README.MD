# GAPShift
THIS NEW CODE IS FOR TESTING. README FILE UNCHANGED.

GAPShift is an experimental reversible transformation for fixed-width integer blocks.

The current implementation works with **4096-bit blocks** and a default test size of **1000 blocks**. It transforms a set of large integer blocks by using the spacing between nearby values, embeds most recovery metadata directly into the encoded data, and can reconstruct the original input exactly.

GAPShift is not an encryption system and it is not a standard compression algorithm. It is a reversible numeric-space transformation experiment.

---

## Current defaults

```text
BIT_WIDTH = 4096
COUNT     = 1000
```

Each block is written as a plain 4096-bit binary string, one block per line.

---

## Core idea

The algorithm works around one value called **GAP**.

For each encode pass:

1. The blocks are sorted in memory.
2. The closest adjacent pair is found.
3. The raw distance is computed:

```text
raw_gap = sorted[i + 1] - sorted[i]
```

4. The operational GAP is computed as:

```text
GAP = raw_gap - 2
```

5. Each sorted block is shifted by rank:

```text
shift(rank) = (rank + 1) * GAP
```

6. The encoded value is:

```text
y = x - shift(rank)
```

The `raw_gap - 2` rule is important because it leaves the selected source pair with an encoded distance of exactly `2`. That gives the decoder a simple recovery anchor:

```text
old_low_output = high_output - 2
```

---

## Alternating view per repeat

Repeat encoding alternates the way values are viewed for sorting and GAP selection.

The actual stored blocks are not inverted. Only the internal comparison view changes:

```text
repeat 1: normal view
repeat 2: inverted bit view
repeat 3: normal view
repeat 4: inverted bit view
...
```

In inverted view, the algorithm behaves as if:

```text
view(x) = (2^4096 - 1) - x
```

This changes the internal ordering between repeat passes without modifying the real block data.

Decode uses the repeat number from the filename and applies the same views in reverse order.

---

## Metadata model

Most metadata is stored inside the first encoded block.

For 1000 blocks, each block index needs 10 bits:

```text
metadata host index : 10 bits
companion index     : 10 bits
inline GAP space    : 4076 bits
```

So the first block stores:

```text
10-bit metadata_host_index + 10-bit companion_index + 4076-bit GAP fragment
```

The metadata host block is moved to the first output position. Its original block index is stored inside that metadata block, so decode can restore the original order.

---

## `metadata.txt`

`metadata.txt` is used only when GAP does not fully fit inside the inline 4076-bit GAP field.

If there is no GAP overflow, no useful metadata line is written for that repeat.

If GAP overflows, only the overflow bits are written.

The file contains **only raw bit lines**:

```text





1
```

Blank lines mean that repeat stage had no external overflow bits. A non-empty line contains only the overflow bits for that repeat stage.

No labels, numbers, or explanatory text are written into `metadata.txt`.

During repeat decode, the decoder reads the line matching the current repeat pass and appends those overflow bits back above the inline GAP fragment.

---

## Repeat encode

Repeat mode can run until the transformation reaches a stopping condition:

```bash
program -r <source_bits_file> <encoded_output_file>
```

Example:

```bash
program -r input.txt packed.txt
```

If the last successful repeat is 6, only the final file is written:

```text
packed.6.txt
```

Intermediate repeat files are not written.

A fixed repeat count can also be requested:

```bash
program -r <repeat_count> <source_bits_file> <encoded_output_file>
```

Example:

```bash
program -r 6 input.txt packed.txt
```

This writes only:

```text
packed.6.txt
```

---

## Repeat decode

Repeat decode uses the repeat number embedded in the filename.

```bash
program -d <repeat_encoded_file> <decoded_output_file>
```

Example:

```bash
program -d packed.6.txt decoded.txt
```

The decoder sees `.6.` in the filename and runs the passes backward:

```text
repeat 6
repeat 5
repeat 4
repeat 3
repeat 2
repeat 1
```

If `metadata.txt` exists, it must be in the same working directory when decoding, because GAP overflow bits may be needed for some repeat stages.

---

## Normal encode

Without arguments, the program generates random unique input blocks and writes the default output:

```bash
program
```

This produces:

```text
input.txt
encode.txt
```

You can also encode a specific source file:

```bash
program -e <source_bits_file> <encoded_output_file>
```

Example:

```bash
program -e input.txt encode.txt
```

---

## Normal decode

Single-pass decode:

```bash
program -d encode.txt decoded.txt
```

If the source filename does not contain a repeat number, decode assumes one pass.

---

## Files produced

### `input.txt`

Original random input blocks as 4096-bit binary strings.

### `encode.txt`

Single-pass encoded output.

### `<name>.<repeat>.txt`

Repeat encoded output. Only the final repeat file is written in repeat mode.

### `metadata.txt`

Optional overflow metadata. It contains only raw bit lines. Blank lines represent repeat stages with no GAP overflow.

### `gap.txt`

Debug information about the selected source pair, raw GAP, GAP, and related encoded values.

### `space.txt`

Detailed debug record of the transformation.

### `smallest_two_outputs.txt`

The two smallest encoded outputs for inspection.

---

## Entropy report

The program prints compact entropy statistics from the in-memory block vectors:

```text
Input entropy
Encoded entropy
Decode source entropy
Restored entropy
```

The report includes:

```text
entropy bits/bit
ones count
zeros count
one percentage
total bit count
```

No bit-output file needs to be read to compute this report.

---

## Proximity report

The program also prints proximity statistics:

```text
min_gap
min_gap/range
avg_gap
avg_gap/range
distance to zero
distance to top
distance ratios
```

The range is:

```text
0 .. 2^4096 - 1
```

This helps show how close the generated or transformed values are to each other, to zero, and to the upper edge of the 4096-bit space.

---

## Reversibility

The design goal is exact recovery.

A successful encode pass performs an internal pre-decode check and prints:

```text
Reversible: true
```

For external validation, compare hashes of the original and decoded files:

```powershell
Compare-Object (Get-FileHash input.txt) (Get-FileHash decoded.txt) -Property Hash
```

An empty result means the files match exactly.

---

## What GAPShift is and is not

GAPShift is:

- deterministic,
- reversible,
- metadata-aware,
- suitable for experiments in numeric-space transformation,
- able to repeat-transform data while preserving decode paths.

GAPShift is not:

- encryption,
- a hash function,
- a security primitive,
- or a standard compression method.

It reorganizes numeric structure while preserving exact recoverability.

---

## Limitations

- A safe closest-pair GAP must exist.
- The rank-based shift must not underflow in the selected view.
- Decode needs the final repeat file.
- Decode also needs `metadata.txt` if any repeat stage used GAP overflow bits.
- The method is structural, not cryptographic.
- Debug reports can be large when detailed files such as `space.txt` are enabled.

---

## Author

Mesut Erturhan

April 20, 2026  
Updated April 21, 2026

---

## License

GPL-3.0 (GNU General Public License v3.0)
