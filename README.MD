# GAPShift  
## A Reversible Data Transformation with Embedded Metadata

## Abstract

GAPShift is a reversible transformation method designed for ordered sets of 256-bit integers.  
The main idea is simple: first, the values are sorted; then a single global step value, called **GAP**, is derived from the closest adjacent pair; after that, each value is shifted downward in a linear and deterministic way.  

The distinctive feature of this version is that the metadata is not kept as a large external record. Instead, one selected block inside the encoded data acts as a **metadata host block**. This block stores the GAP value together with the position of its companion block. A small external file, `metadata.txt`, stores only the original position of the metadata host block.  

This design keeps the system reversible while reducing external metadata cost. The transformation is not intended to compress entropy, encrypt data, or hide information cryptographically. Its purpose is to reorganize numerical data inside the available data space in a controlled and fully recoverable way.

---

## 1. Introduction

Suppose we have many large numbers, each exactly 256 bits wide.  
We want to move these numbers to new positions in the numeric space, but we do not want to destroy them. After the transformation, we must still be able to recover the original values exactly.

At first glance, this sounds easy: subtract some amount from each value, save that amount, and later add it back.  
However, the real challenge is not subtraction itself. The real challenge is to do this:

1. with a single global rule,  
2. without storing large metadata for every block,  
3. while keeping the method reversible.

GAPShift solves this by selecting a meaningful local structure inside the sorted data: the **closest adjacent pair**.  
From that pair, it builds a single transformation step called **GAP**, and then applies a linear rank-based shift to the whole sorted dataset.

---

## 2. General Idea

The method has four main phases:

1. **Sort the values**
2. **Find the closest adjacent pair**
3. **Transform all values using one GAP**
4. **Embed the metadata into one block**

The reverse process performs the same reasoning in the opposite direction:

1. **Find the metadata host block**
2. **Recover GAP and the companion block position**
3. **Restore the selected pair to its old encoded form**
4. **Decode all values normally**

This may sound technical at first, but the logic is quite natural:

- first find the tightest gap,
- turn that gap into a usable step,
- apply the same step consistently,
- keep just enough information to reverse the process.

---

## 3. Data Model

Each record is treated as a 256-bit unsigned integer.

Let the sorted input be:

```text
x[0] < x[1] < x[2] < ... < x[n-1]
```

Here:

- `x[i]` is the value at sorted rank `i`
- `n` is the number of records
- all values are unique

The transformation works on the **sorted order**, but the final output files are written in the **original block order**.

This distinction is important:

- the mathematics of the transformation is defined on sorted ranks,
- the stored `input.txt` and `encode.txt` preserve the original block ordering.

---

## 4. GAP Selection

The system first searches the sorted sequence for the **closest adjacent pair**.

For every adjacent pair:

```text
raw_gap(i) = x[i+1] - x[i]
```

The minimum of these values is selected:

```text
raw_gap = min(x[i+1] - x[i])
```

If the closest pair is:

```text
x[k], x[k+1]
```

then this pair becomes the **source pair**.

The operational GAP is then defined as:

```text
GAP = raw_gap - 2
```

This “minus 2” rule is intentional.  
It ensures that, after encoding, the selected pair becomes separated by exactly 2 in the encoded space.

That property is later used during decoding.

---

## 5. Why “raw_gap - 2”?

This rule can be understood with a small example.

Suppose the closest adjacent pair is:

```text
15, 20
```

Then:

```text
raw_gap = 20 - 15 = 5
GAP = 5 - 2 = 3
```

If these values are encoded with consecutive linear shifts, their encoded distance becomes:

```text
2
```

This is not accidental. It is the reason the system can later restore the metadata host block using the rule:

```text
old_low_output = high_output - 2
```

So the “minus 2” is not an arbitrary tweak.  
It is the structural bridge between encode and decode.

---

## 6. Linear Encoding Rule

After GAP has been chosen, every sorted value receives a shift based on its rank.

For sorted rank `i`:

```text
shift(i) = (i + 1) * GAP
```

Then the encoded value is:

```text
y[i] = x[i] - shift(i)
```

This is a **linear, rank-based transformation**.

In plain language:

- the smallest sorted value loses `1 * GAP`
- the second loses `2 * GAP`
- the third loses `3 * GAP`
- and so on

This means later values are moved further down than earlier ones.

---

## 7. Underflow Safety

A valid transformation must not push any value below zero.

Therefore, before encoding, the system checks:

```text
x[i] >= (i + 1) * GAP
```

for every sorted rank `i`.

If this condition fails for any value, the transformation is rejected.  
This prevents underflow and protects reversibility.

---

## 8. Metadata Embedding

A major goal of this design is to reduce metadata cost.

Instead of storing all metadata outside the encoded dataset, the method selects one block from the source pair as a **metadata host block**.

### 8.1 Which block becomes the host?

The lower member of the selected source pair becomes the metadata host block.

The higher member becomes the **companion block**.

### 8.2 What does the host block store?

The host block stores two things:

1. the recovered operational `GAP`
2. the original index of the companion block

These two values are packed into the same 256-bit block.

The host block therefore no longer stores its ordinary encoded value.  
It becomes a metadata carrier.

### 8.3 What is stored in `metadata.txt`?

The file `metadata.txt` stores only one thing:

- the **original index of the metadata host block**

This index is stored with a dynamic bit width:

```text
position_bits = ceil(log2(COUNT))
```

So if the system contains more blocks, the metadata position field grows automatically.

For example:

- `COUNT = 10,000` requires `14` position bits
- `COUNT = 5,000,000` requires `23` position bits

This is why the design is scalable.

---

## 9. Metadata Packing

If `position_bits` is the number of bits needed to represent a block position, then the host block is packed as:

```text
packed_metadata_block = [GAP][companion_position]
```

More precisely:

- the lower `position_bits` bits store the companion block position
- the remaining upper bits store the GAP value

This means the host block does **not** store its own position.  
Its position is already known from `metadata.txt`.  
So there is no need to waste extra space by writing the same location twice.

This makes the metadata representation compact and efficient.

---

## 10. Output Order

The transformation is computed in sorted order, but the exported files preserve the **original input order**.

That means:

- `input.txt` stores the original data in original block order
- `encode.txt` stores encoded data in original block order

This is important because the block positions used by the metadata system refer to the real block layout, not to a rewritten sorted file layout.

---

## 11. Decoding Process

The decode phase works in four steps.

### Step 1 — Read the metadata host position

The system reads `metadata.txt` and finds the original block position of the metadata host.

### Step 2 — Read the metadata host block

From the host block, the system extracts:

- `GAP`
- companion block position

### Step 3 — Restore the selected source pair

Because the metadata host block was overwritten, its old encoded value must be reconstructed.

Let the companion block’s encoded value be the higher output of the selected pair.  
Then the old encoded value of the metadata host block is:

```text
old_low_output = high_output - 2
```

This works because the selected source pair was constructed so that its old encoded difference is exactly 2.

### Step 4 — Perform normal decode

Once the pair is restored, the system returns to the standard linear inverse rule:

```text
x[i] = y[i] + (i + 1) * GAP
```

This reproduces the original sorted values, which are then mapped back to the original block order.

---

## 12. A Small Example

Consider this sorted input:

```text
15
20
39
```

The closest adjacent pair is:

```text
15, 20
```

So:

```text
raw_gap = 20 - 15 = 5
GAP = 5 - 2 = 3
```

Now encode linearly:

```text
15 -> 12
20 -> 14
39 -> 30
```

The selected pair difference in encoded form is:

```text
14 - 12 = 2
```

Now suppose the lower block of that pair becomes the metadata host block.  
Its ordinary encoded value is replaced by metadata.

During decoding:

- the host block reveals the GAP and the companion position
- the companion block gives the higher encoded output
- the old encoded host output is restored as:

```text
high_output - 2
```

Then normal decode proceeds.

This example is small, but the same logic scales to large 256-bit datasets.

---

## 13. Files Produced by the Program

### `input.txt`
Stores the original values as pure 256-bit binary strings, one per line, in original block order.

### `encode.txt`
Stores the encoded values as pure 256-bit binary strings, one per line, in original block order.

### `metadata.txt`
Stores only the original position of the metadata host block.  
Its width is dynamic and depends on the number of blocks.

### `gap.txt`
Stores technical information about the selected source pair, `raw_gap`, `GAP`, source pair positions, and related encoded values.

### `space.txt`
Stores a detailed record of the transformation, including original values, encoded values, decoded values, shifts, and block roles.

### `smallest_two_outputs.txt`
Stores the two smallest encoded values for inspection.

---

## 14. Decode Mode

The program supports decode mode:

```bash
program -d <source_bits_file> <decoded_output_file>
```

Example:

```bash
program -d encode.txt decoded.txt
```

Decode mode performs the following operations:

1. reads the metadata host position from `metadata.txt`
2. reads the metadata host block from the source file
3. extracts GAP and companion position
4. restores the overwritten source-pair value
5. performs normal decode
6. writes the restored values to the output file

---

## 15. Properties of the Method

This version of GAPShift has the following properties:

- **Deterministic**  
  The same input always produces the same output.

- **Reversible**  
  If the transformation succeeds, the original data can be recovered exactly.

- **Compact external metadata**  
  The external metadata is reduced to a single block position.

- **Embedded control data**  
  The host block carries both GAP and companion location.

- **Scalable position encoding**  
  Position metadata expands automatically with the number of blocks.

- **Not cryptographic**  
  The method is not encryption and should not be treated as a security primitive.

---

## 16. What This System Is Not

This project is **not**:

- a compression algorithm in the classical entropy-reduction sense
- a cryptographic cipher
- a hash function
- a randomization engine

Its purpose is structural and positional transformation.  
It reorganizes data in numeric space while preserving exact recoverability.

---

## 17. Limitations

This method still has important limits:

1. It depends on a valid GAP source pair.
2. It requires underflow safety to hold for all sorted ranks.
3. It is not designed to hide information against adversaries.
4. It relies on the metadata host block remaining accessible and unchanged.
5. Decode in the present implementation uses the original ordering model of the dataset.

These are not flaws, but design boundaries.

---

## 18. Conclusion

GAPShift demonstrates that a set of large integers can be repositioned through a simple linear rule while remaining exactly reversible.  
The method becomes especially interesting when metadata is embedded directly into one of the encoded blocks, leaving only a small external positional hint.

In simple terms, the system does three things:

1. finds the tightest local gap,
2. turns that gap into a global reversible step,
3. hides the key recovery information inside the data itself.

This makes the method structurally elegant, compact in metadata usage, and suitable for controlled experiments in reversible data-space transformation.

---

## Author

Mesut Erturhan  
April 20, 2026 — Updated April 21, 2026

---

## License

GPL-3.0 (GNU General Public License v3.0)
