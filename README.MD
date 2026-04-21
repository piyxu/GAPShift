# GAPShift: A Reversible Data Transformation
#
## Overview
This is an experimental research study that I am currently conducting. There may be errors in the code.

This project implements a reversible transformation over a set of 256-bit integers by **repositioning values within the numeric space using a deterministic GAP-based shifting system**.

Unlike the previous version (D-based), this system:

* Uses a **single global GAP value**
* Applies shifts only starting from a configurable index (`START_INDEX`)
* Preserves ordering
* Enables full reversibility using only GAP

---

## Core Idea

### Encoding

Values are sorted in ascending order.

For each value:

```
shift(i) = (i - START_INDEX + 1) * GAP   if i >= START_INDEX
shift(i) = 0                             if i < START_INDEX
```

```
y = x - shift(i)
```

Where:

* `x` = original value
* `y` = encoded value
* `i` = index in ascending order
* `GAP` = global metadata
* `START_INDEX` = where shifting begins

---

### Decoding

```
x = y + shift(i)
```

The same shift is reconstructed deterministically using GAP and index.

---

## Key Property

If ordering is preserved:

```
y[i] < y[i+1] for all i
```

Then:

```
decoded == original
```

---

## GAP Selection

GAP is derived from the **active region only** (starting at `START_INDEX`):

```
active_min_gap = min(sorted[i] - sorted[i-1]) for i >= START_INDEX
```

Then:

```
gap_by_order = active_min_gap - 2
gap_by_value = start_value / active_count

GAP = min(gap_by_order, gap_by_value)
```

Where:

* `start_value = sorted[START_INDEX]`
* `active_count = COUNT - START_INDEX`

---

## Safety Constraints

1. Order preservation:

```
GAP < active_min_gap
```

2. No underflow:

```
active_count * GAP <= start_value
```

---

## System Behavior

* The smallest values (before `START_INDEX`) remain unchanged
* Each subsequent value is shifted more than the previous one
* Shift grows linearly: `+GAP` per step

### Example

```
Index:        0    1    2    3
Values:      10   15   20   25
START_INDEX = 1
GAP = 3

Encoded:
10 -> 10
15 -> 12   (-3)
20 -> 14   (-6)
25 -> 16   (-9)
```

---

## Output Files

### metadata.txt

Contains the GAP value in 256-bit binary form.

### space.txt

Detailed dataset including:

* original values
* encoded values
* decoded values
* bit representations
* applied shifts

### smallest_two_outputs.txt

Displays the two smallest encoded values.

### gap.txt

Shows smallest encoded values and their difference.

### input.txt

Raw input values as pure 256-bit binary strings (one per line).

### encode.txt

Encoded values as pure 256-bit binary strings (one per line).

---

## Decode Mode

You can decode encoded bit files using:

```
program -d <source_bits_file> <output_file>
```

### Example

```
program -d encode.txt decoded.txt
```

### Requirements

* `metadata.txt` must exist
* Bit width must match

---

## Notes

* Fully reversible transformation
* Deterministic (no randomness in encoding phase)
* Not a cryptographic system
* Designed for structural data repositioning experiments
* Entropy is preserved; only spatial distribution changes

---

## Author

Mesut Erturhan
April 20, 2026 - Update 21-04-2026

---

## License

GPL-3.0 (GNU General Public License v3.0)
