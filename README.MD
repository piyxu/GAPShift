# Repositioning Information in the Data Space

## Overview

This project explores a transformation technique that repositions information within a numerical space while preserving its structure and enabling full reversibility using a single metadata value.

Given a set of 256-bit integers, we:

- Reorder them in descending order
- Apply deterministic shifts based on a global parameter D
- Encode them into a new space
- Recover the original values using only D

---

## Core Idea

### Encoding

`y = x - shift(i) ` 
`shift(i) = (i + 1) * D`

Where:
- x = original value
- y = encoded value
- i = index in descending order
- D = global metadata

---

### Decoding

`x = y + shift(i)`

No per-record metadata is required.  
All shifts are reconstructed deterministically from D.

---

## Key Property

If ordering is preserved after encoding, the transformation is fully reversible.

`order_ok = y[i] > y[i+1] for all i`

`If order_ok == true:`

`decoded == original`

---

## Metadata (D)

`D = min((min_gap // 2) - 2, min_value // COUNT)`

Where:
- min_gap = smallest difference between sorted input values
- min_value = smallest input value
- COUNT = number of elements

---

## Safety Constraints

1) No overlap (ordering preserved):
`D < min_gap`

2) No underflow `(stay >= 0):`
`COUNT * D <= min_value`

---

## Output Files

### metadata.txt
Contains only the metadata D in 256-bit binary form.

### space.txt
Full dataset including original, encoded, decoded values and bit representations.

### smallest_two_outputs.txt
Displays the two smallest encoded values.

### gap.txt
Displays smallest values and their difference.

---

## Notes

- Experimental project
- Not a cryptographic system
- Behavior depends on input distribution
- This system does not violate Shannon's entropy; it merely optimizes the noise within the addressing space.

---

## Author

Mesut Erturhan  
April 20, 2026

---

## License

GPL-3.0 (GNU General Public License v3.0
