// GAPShift: A Reversible Data Transformation
// © 2026 Mesut Erturhan / GITHUB PIYXU
// Licensed under the GNU General Public License v3.0.

// Code implementation and enhancement assisted by ChatGPT.
// Core principles developed in collaboration with Gemini.
// https://github.com/piyxu/GAPShift/
// GAPShift Version Update Version 0.1.2 Test 
// GAPShift pipeline overview
//
// 1. Input blocks are fixed-width 4096-bit unsigned integers.
// 2. Each encode pass sorts blocks in an internal view. Odd repeats use the
//    real value; even repeats use the bit-inverted view `(2^4096 - 1) - x`.
//    The stored values themselves are not inverted.
// 3. The closest adjacent pair in that view defines `raw_gap`.
// 4. The operational GAP is `raw_gap - 2`. This makes the selected encoded
//    pair differ by exactly 2, so the overwritten metadata-host output can be
//    restored during decode with `companion_output - 2`.
// 5. Every sorted block is shifted by `(rank + 1) * GAP`, then written back in
//    original block order.
// 6. Experimental self-sum metadata mode keeps the metadata host in its own
//    original slot. The host no longer stores its own index.
// 7. The metadata payload stores only the source companion index and GAP:
//    `payload = GAP * COUNT + companion_index`.
// 8. The payload is added to the largest encoded value excluding the metadata
//    host, then written into the metadata host slot. If this addition would
//    exceed the fixed block width, the pass stops; metadata.txt is not used.
// 9. Decode finds the largest output as the metadata host, subtracts the next
//    largest output to recover the payload, restores the host from
//    `companion_output - 2`, then reverses the rank shifts.
// 10. The final decoded output should match the original input exactly.

use num_bigint::{BigUint, RandBigInt};
use num_traits::Zero;
use rand::rngs::OsRng;
use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_BIT_WIDTH: u64 = 4096;
const DEFAULT_COUNT: usize = 1000;

#[derive(Clone, Copy, Debug)]
struct Config {
    bit_width: u64,
    count: usize,
    bit_width_explicit: bool,
}

static CONFIG: OnceLock<Config> = OnceLock::new();
static ACTIVE_BIT_WIDTH: AtomicU64 = AtomicU64::new(0);

fn config() -> &'static Config {
    CONFIG.get().expect("Runtime config was not initialized.")
}

fn bit_width() -> u64 {
    config().bit_width
}

fn active_bit_width() -> u64 {
    let active = ACTIVE_BIT_WIDTH.load(Ordering::Relaxed);
    if active == 0 {
        bit_width()
    } else {
        active
    }
}

fn max_value_bit_width(values: &[BigUint]) -> u64 {
    values
        .iter()
        .map(|value| value.bits())
        .max()
        .unwrap_or(0)
        .max(bit_width())
        .max(1)
}

fn set_active_bit_width(width: u64) {
    ACTIVE_BIT_WIDTH.store(width.max(bit_width()).max(1), Ordering::Relaxed);
}
const OUTPUT_FILE: &str = "space.txt";
const SMALLEST_TWO_OUTPUTS_FILE: &str = "smallest_two_outputs.txt";
const GAP_FILE: &str = "gap.txt";
const INPUT_BITS_FILE: &str = "input.txt";
const ENCODE_BITS_FILE: &str = "encode.txt";
const BITS_FILE: &str = "bits.txt";

// A sorted value keeps both the real block (`x`) and the comparison key.
// The key may be the bit-inverted view used on even repeat passes.
#[derive(Clone, Debug)]
struct SortedValue {
    sorted_rank: usize,
    orig_index: usize,
    x: BigUint,
    key: BigUint,
}

#[derive(Clone, Debug)]
struct EncodedRecord {
    sorted_rank: usize,
    x: BigUint,
    shift: BigUint,
    is_metadata_host: bool,
    is_source_companion: bool,
}

struct EntropyStats {
    total_bits: u64,
    ones: u64,
    zeros: u64,
    one_ratio: f64,
    entropy_per_bit: f64,
}

#[derive(Clone, Debug)]
struct LeadingOnesReport {
    repeat: usize,
    leading_ones: u64,
    largest_value: BigUint,
}

// All persisted block files use fixed-width binary strings.
fn to_bits(n: &BigUint, width: u64) -> String {
    let s = n.to_str_radix(2);
    if s.len() >= width as usize {
        s
    } else {
        let mut out = String::with_capacity(width as usize);
        out.push_str(&"0".repeat(width as usize - s.len()));
        out.push_str(&s);
        out
    }
}

fn leading_ones_count(value: &BigUint, width: u64) -> u64 {
    to_bits(value, width)
        .bytes()
        .take_while(|bit| *bit == b'1')
        .count() as u64
}

fn largest_value(values: &[BigUint]) -> Option<BigUint> {
    values.iter().cloned().max()
}

fn collect_leading_ones_report(repeat: usize, values: &[BigUint]) -> Option<LeadingOnesReport> {
    largest_value(values).map(|largest_value| LeadingOnesReport {
        repeat,
        leading_ones: leading_ones_count(&largest_value, active_bit_width()),
        largest_value,
    })
}

fn prefix_bits(value: &BigUint, width: u64, take: usize) -> String {
    let bits = to_bits(value, width);
    bits.chars().take(take).collect()
}

fn write_bits_report(reports: &[LeadingOnesReport]) -> Result<(), Box<dyn std::error::Error>> {
    if reports.is_empty() {
        match fs::remove_file(BITS_FILE) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
        return Ok(());
    }

    let file = File::create(BITS_FILE)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "DATE                         : 2026-04-26")?;
    writeln!(writer, "BIT_WIDTH                    : {}", active_bit_width())?;
    writeln!(writer, "FIELD                        : leading 1-bit run of the largest encoded value after each repeat")?;
    writeln!(writer, "RULE                         : prefix 1110 => 3, prefix 1111... => run length until first 0")?;
    writeln!(writer)?;

    for report in reports {
        writeln!(writer, "REPEAT                       : {}", report.repeat)?;
        writeln!(writer, "LARGEST_LEADING_ONES         : {}", report.leading_ones)?;
        writeln!(writer, "LARGEST_VALUE                : {}", summarize_biguint(&report.largest_value))?;
        writeln!(writer, "LARGEST_PREFIX_BITS          : {}", prefix_bits(&report.largest_value, active_bit_width(), 64))?;
        writeln!(writer, "{}", "-".repeat(80))?;
    }

    if let Some(min_report) = reports.iter().min_by_key(|report| report.leading_ones) {
        writeln!(writer)?;
        writeln!(writer, "MIN_LEADING_ONES             : {}", min_report.leading_ones)?;
        writeln!(writer, "MIN_LEADING_ONES_REPEAT      : {}", min_report.repeat)?;
        writeln!(writer, "MIN_LEADING_ONES_PREFIX      : {}", prefix_bits(&min_report.largest_value, bit_width(), 64))?;
    }

    if let Some(max_report) = reports.iter().max_by_key(|report| report.leading_ones) {
        writeln!(writer, "MAX_LEADING_ONES             : {}", max_report.leading_ones)?;
        writeln!(writer, "MAX_LEADING_ONES_REPEAT      : {}", max_report.repeat)?;
        writeln!(writer, "MAX_LEADING_ONES_PREFIX      : {}", prefix_bits(&max_report.largest_value, bit_width(), 64))?;
    }

    Ok(())
}

fn detect_bit_width_from_file(path: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.bytes().all(|b| b == b'0' || b == b'1') {
            return Err(format!("Invalid character in bit string while detecting bit width. line={}", line_no + 1).into());
        }
        return Ok(trimmed.len() as u64);
    }

    Err(format!("Cannot detect bit width from empty source file: {}", path).into())
}

fn bit_count_ones(value: &BigUint) -> u64 {
    value
        .to_u64_digits()
        .iter()
        .map(|digit| digit.count_ones() as u64)
        .sum()
}

fn entropy_stats(values: &[BigUint], width: u64) -> EntropyStats {
    let total_bits = values.len() as u64 * width;
    let ones: u64 = values.iter().map(bit_count_ones).sum();
    let zeros = total_bits.saturating_sub(ones);
    let one_ratio = if total_bits == 0 {
        0.0
    } else {
        ones as f64 / total_bits as f64
    };
    let zero_ratio = 1.0 - one_ratio;
    let entropy_per_bit = [one_ratio, zero_ratio]
        .into_iter()
        .filter(|p| *p > 0.0)
        .map(|p| -p * p.log2())
        .sum();

    EntropyStats {
        total_bits,
        ones,
        zeros,
        one_ratio,
        entropy_per_bit,
    }
}

fn print_entropy(label: &str, values: &[BigUint]) {
    let stats = entropy_stats(values, active_bit_width());
    println!(
        "{} entropy              : {:.6} bits/bit, ones={} ({:.4}%), zeros={}, total_bits={}",
        label,
        stats.entropy_per_bit,
        stats.ones,
        stats.one_ratio * 100.0,
        stats.zeros,
        stats.total_bits
    );
}

fn summarize_scaled_ratio(numerator: &BigUint, denominator: &BigUint) -> String {
    if denominator.is_zero() {
        return "undefined".to_string();
    }

    let scale = BigUint::from(100_000_000u64);
    let scaled_percent = (numerator * scale) / denominator;
    let digits = scaled_percent.to_u64_digits();
    let value = if digits.is_empty() { 0 } else { digits[0] };
    let whole = value / 1_000_000;
    let frac = value % 1_000_000;
    format!("{}.{:06}%", whole, frac)
}

fn print_proximity(label: &str, values: &[BigUint]) {
    if values.is_empty() {
        println!("{} proximity           : empty", label);
        return;
    }

    let max_value = bit_mask();
    let mut sorted = values.to_vec();
    sorted.sort();

    let min_value = &sorted[0];
    let max_seen = &sorted[sorted.len() - 1];
    let distance_to_top = &max_value - max_seen;

    let mut min_gap: Option<BigUint> = None;
    let mut total_gap = BigUint::zero();
    for window in sorted.windows(2) {
        let gap = &window[1] - &window[0];
        if min_gap.as_ref().map_or(true, |current| gap < *current) {
            min_gap = Some(gap.clone());
        }
        total_gap += gap;
    }

    let avg_gap = if sorted.len() > 1 {
        total_gap / BigUint::from((sorted.len() - 1) as u64)
    } else {
        BigUint::zero()
    };
    let min_gap = min_gap.unwrap_or_else(BigUint::zero);

    println!(
        "{} proximity            : min_gap={}, min_gap/range={}, avg_gap={}, avg_gap/range={}",
        label,
        summarize_biguint(&min_gap),
        summarize_scaled_ratio(&min_gap, &max_value),
        summarize_biguint(&avg_gap),
        summarize_scaled_ratio(&avg_gap, &max_value)
    );
    println!(
        "{} edge distance         : to_zero={}, to_zero/range={}, to_top={}, to_top/range={}",
        label,
        summarize_biguint(min_value),
        summarize_scaled_ratio(min_value, &max_value),
        summarize_biguint(&distance_to_top),
        summarize_scaled_ratio(&distance_to_top, &max_value)
    );
}

fn summarize_biguint(n: &BigUint) -> String {
    let decimal = n.to_str_radix(10);
    let prefix_len = decimal.len().min(18);
    let suffix_len = decimal.len().min(18);
    format!(
        "bits={}, digits={}, prefix={}, suffix={}",
        n.bits(),
        decimal.len(),
        &decimal[..prefix_len],
        &decimal[decimal.len() - suffix_len..]
    )
}

fn summarize_gap_ratio(gap: &BigUint, raw_gap: &BigUint) -> String {
    if raw_gap.is_zero() {
        return "undefined".to_string();
    }

    let scale = BigUint::from(100_000_000u64);
    let scaled_percent = (gap * scale) / raw_gap;
    let digits = scaled_percent.to_u64_digits();
    let value = if digits.is_empty() { 0 } else { digits[0] };
    let whole = value / 1_000_000;
    let frac = value % 1_000_000;
    format!("{}.{:06}%", whole, frac)
}

fn clear_legacy_metadata_file() -> Result<(), Box<dyn std::error::Error>> {
    // This experiment does not use metadata.txt. Remove stale files so a run is
    // self-contained and decode never depends on external overflow metadata.
    match fs::remove_file("metadata.txt") {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn from_bits(s: &str) -> Result<BigUint, String> {
    BigUint::parse_bytes(s.trim().as_bytes(), 2)
        .ok_or_else(|| format!("Invalid bit string: {}", s.trim()))
}

fn bits_needed_for_count(count: usize) -> u64 {
    if count <= 1 {
        1
    } else {
        (usize::BITS - (count - 1).leading_zeros()) as u64
    }
}

fn companion_space(count: usize) -> Result<BigUint, String> {
    if count < 2 {
        return Err("At least 2 blocks are required for companion metadata.".to_string());
    }
    Ok(BigUint::from(count as u64))
}

fn bits_needed_for_states(states: &BigUint) -> u64 {
    if states.is_zero() {
        return 1;
    }
    let one = BigUint::from(1u32);
    if *states <= one {
        1
    } else {
        (states - one).bits()
    }
}

fn metadata_gap_bits_for_count(count: usize) -> Result<u64, String> {
    let companion_space = companion_space(count)?;
    let companion_bits = bits_needed_for_states(&companion_space);
    active_bit_width()
        .checked_sub(companion_bits)
        .ok_or_else(|| format!("bit_width {} is too small for {} companion metadata bits.", active_bit_width(), companion_bits))
}

fn pack_companion_gap(
    gap: &BigUint,
    companion_orig_index: usize,
    count: usize,
) -> Result<BigUint, String> {
    if companion_orig_index >= count {
        return Err(format!(
            "Companion position {} does not fit in count {}.",
            companion_orig_index, count
        ));
    }

    // Self-sum metadata packing, old-style mixed-radix:
    // metadata_block = GAP * COUNT + companion_index
    // The metadata host index is not stored. Decode identifies it as the
    // largest block after metadata embedding.
    let metadata_block = gap * BigUint::from(count as u64) + BigUint::from(companion_orig_index as u64);

    // Experimental self-sum mode: do not reject a metadata block just because
    // it is wider than the configured/original block width. The whole point of
    // this experiment is to let the self-sum value grow and observe repeat
    // behavior with a dynamically widened working width.
    Ok(metadata_block)
}

fn unpack_companion_gap(
    metadata_block: &BigUint,
    count: usize,
) -> Result<(usize, BigUint), String> {
    let base = BigUint::from(count as u64);
    let companion = biguint_to_usize(metadata_block % &base);
    let gap = metadata_block / &base;

    if companion >= count {
        return Err(format!(
            "Decoded companion index {} is out of range for count {}.",
            companion, count
        ));
    }
    if gap.is_zero() {
        return Err("Recovered GAP is zero.".to_string());
    }

    Ok((companion, gap))
}


fn biguint_to_usize(value: BigUint) -> usize {
    let digits = value.to_u64_digits();
    if digits.is_empty() {
        0
    } else {
        digits[0] as usize
    }
}

fn bit_mask() -> BigUint {
    (BigUint::from(1u32) << active_bit_width()) - 1u32
}

// Even repeat passes do not mutate stored data; they only sort and compute
// through the complementary 4096-bit view.
fn use_inverted_view(_repeat: usize) -> bool {
    // Experimental self-sum mode: always use the normal view.
    // This disables the previous even-repeat inverted read path.
    false
}

fn view_value(value: &BigUint, inverted: bool) -> Result<BigUint, String> {
    if inverted {
        let mask = bit_mask();
        if value > &mask {
            return Err(format!(
                "Value exceeds {} bit view mask. value_bits={}",
                active_bit_width(),
                value.bits()
            ));
        }
        Ok(mask - value)
    } else {
        Ok(value.clone())
    }
}

fn rand_bits(bit_width: u64, rng: &mut OsRng) -> BigUint {
    rng.gen_biguint(bit_width)
}

fn generate_unique_values(count: usize, bit_width: u64) -> Vec<BigUint> {
    let mut rng = OsRng;
    let mut seen: HashSet<BigUint> = HashSet::with_capacity(count);
    let mut out = Vec::with_capacity(count);

    while out.len() < count {
        let v = rand_bits(bit_width, &mut rng);
        if seen.insert(v.clone()) {
            out.push(v);
        }
    }

    out
}

fn validate_unique_values(values: &[BigUint]) -> Result<(), String> {
    let mut seen: HashSet<BigUint> = HashSet::with_capacity(values.len());

    for (i, value) in values.iter().enumerate() {
        if !seen.insert(value.clone()) {
            return Err(format!("Duplicate input value at line/index {}.", i + 1));
        }
    }

    Ok(())
}

fn sorted_values(values: &[BigUint], inverted_view: bool) -> Result<Vec<SortedValue>, String> {
    let mut pairs: Vec<(usize, BigUint, BigUint)> = Vec::with_capacity(values.len());
    for (orig_index, x) in values.iter().cloned().enumerate() {
        let key = view_value(&x, inverted_view)?;
        pairs.push((orig_index, x, key));
    }
    pairs.sort_by_key(|(_, _, key)| key.clone());

    Ok(pairs
        .into_iter()
        .enumerate()
        .map(|(sorted_rank, (orig_index, x, key))| SortedValue {
            sorted_rank,
            orig_index,
            x,
            key,
        })
        .collect())
}

// The closest adjacent pair in the current view provides the recovery anchor.
// Subtracting 2 makes that selected encoded pair differ by exactly 2.
fn compute_gap_source(sorted: &[SortedValue]) -> Result<(usize, usize, BigUint, BigUint), String> {
    if sorted.len() < 2 {
        return Err("At least 2 numbers are required.".to_string());
    }

    let mut best_left = 0usize;
    let mut best_right = 1usize;
    let mut best_raw_gap = &sorted[1].key - &sorted[0].key;

    for i in 1..sorted.len() - 1 {
        let diff = &sorted[i + 1].key - &sorted[i].key;
        if diff < best_raw_gap {
            best_raw_gap = diff;
            best_left = i;
            best_right = i + 1;
        }
    }

    let delta = BigUint::from(2u32);
    if best_raw_gap <= delta {
        return Err(format!(
            "Safe GAP cannot be produced. raw_gap={} <= 2",
            best_raw_gap
        ));
    }

    let gap = &best_raw_gap - 2u32;

    for (i, sv) in sorted.iter().enumerate() {
        let shift = BigUint::from((i + 1) as u64) * &gap;
        if sv.key < shift {
            return Err(format!(
                "Underflow risk at sorted rank {}: value {} is less than shift {}",
                i, sv.key, shift
            ));
        }
    }

    Ok((best_left, best_right, best_raw_gap, gap))
}

fn build_shifts(count: usize, gap: &BigUint) -> Vec<BigUint> {
    (0..count)
        .map(|i| BigUint::from((i + 1) as u64) * gap)
        .collect()
}

fn encode_sorted(
    sorted: &[SortedValue],
    shifts: &[BigUint],
    source_left_sorted_pos: usize,
    source_right_sorted_pos: usize,
    inverted_view: bool,
) -> Result<(Vec<BigUint>, Vec<EncodedRecord>, bool), String> {
    let mut outputs = Vec::with_capacity(sorted.len());
    let mut records = Vec::with_capacity(sorted.len());

    for (sv, shift) in sorted.iter().zip(shifts.iter()) {
        if sv.key < *shift {
            return Err(format!("Encode failed: key < shift. key={}, shift={}", sv.key, shift));
        }

        // Encode in view-space, then convert back to the real stored value.
        let y_key = &sv.key - shift;
        outputs.push(view_value(&y_key, inverted_view)?);
        records.push(EncodedRecord {
            sorted_rank: sv.sorted_rank,
            x: sv.x.clone(),
            shift: shift.clone(),
            is_metadata_host: sv.sorted_rank == source_left_sorted_pos,
            is_source_companion: sv.sorted_rank == source_right_sorted_pos,
        });
    }

    let mut order_ok = true;
    for w in outputs.windows(2) {
        if view_value(&w[0], inverted_view)? >= view_value(&w[1], inverted_view)? {
            order_ok = false;
            break;
        }
    }
    Ok((outputs, records, order_ok))
}

fn reorder_to_original_order<T: Clone>(sorted: &[SortedValue], values_in_sorted_order: &[T]) -> Vec<T> {
    let mut pairs: Vec<(usize, T)> = sorted
        .iter()
        .zip(values_in_sorted_order.iter().cloned())
        .map(|(sv, value)| (sv.orig_index, value))
        .collect();
    pairs.sort_by_key(|(orig_index, _)| *orig_index);
    pairs.into_iter().map(|(_, value)| value).collect()
}

fn largest_excluding_index(values: &[BigUint], excluded_index: usize) -> Result<BigUint, String> {
    values
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != excluded_index)
        .map(|(_, value)| value.clone())
        .max()
        .ok_or_else(|| "Cannot find largest value excluding metadata host.".to_string())
}

fn largest_two_indices(values: &[BigUint]) -> Result<((usize, BigUint), (usize, BigUint)), String> {
    if values.len() < 2 {
        return Err("At least 2 blocks are required to recover self-sum metadata.".to_string());
    }

    let mut top: Option<(usize, BigUint)> = None;
    let mut second: Option<(usize, BigUint)> = None;

    for (index, value) in values.iter().cloned().enumerate() {
        match &top {
            None => top = Some((index, value)),
            Some((_, top_value)) if value > *top_value => {
                second = top.take();
                top = Some((index, value));
            }
            _ => match &second {
                None => second = Some((index, value)),
                Some((_, second_value)) if value > *second_value => {
                    second = Some((index, value));
                }
                _ => {}
            },
        }
    }

    Ok((
        top.ok_or_else(|| "Missing largest metadata block.".to_string())?,
        second.ok_or_else(|| "Missing base block below metadata block.".to_string())?,
    ))
}

fn embed_metadata_into_host(
    outputs_in_original_order: &mut [BigUint],
    metadata_host_orig_index: usize,
    gap: &BigUint,
    source_companion_orig_index: usize,
    count: usize,
) -> Result<(), String> {
    let metadata_block = pack_companion_gap(gap, source_companion_orig_index, count)?;
    let base = largest_excluding_index(outputs_in_original_order, metadata_host_orig_index)?;
    let new_metadata_value = &base + &metadata_block;

    // Experimental mode: self-sum width checks are temporarily disabled and output width may grow.
    // If base + metadata_block exceeds the configured bit width, the value is
    // still written as-is so the behavior can be observed without changing any
    // other parameters.

    // Keep the metadata host in its original output slot. No slot-0 swap and no
    // host-index metadata are used in this experiment.
    outputs_in_original_order[metadata_host_orig_index] = new_metadata_value;
    Ok(())
}

fn restore_selected_pair_before_decode(
    outputs_in_original_order: &mut [BigUint],
    count: usize,
    inverted_view: bool,
) -> Result<(BigUint, usize, usize), String> {
    if outputs_in_original_order.len() < 2 {
        return Err("Decode source must contain at least 2 blocks.".to_string());
    }

    // The metadata host was made the largest real stored value by adding the
    // companion+GAP metadata block to the largest non-host encoded value. The next
    // largest value therefore recovers the additive base.
    let ((metadata_host_orig_index, metadata_value), (_, base_value)) =
        largest_two_indices(outputs_in_original_order)?;

    if metadata_value <= base_value {
        return Err("Invalid self-sum metadata ordering: largest <= second largest.".to_string());
    }

    let metadata_block = &metadata_value - &base_value;
    let (source_companion_orig_index, gap) = unpack_companion_gap(&metadata_block, count)?;

    if metadata_host_orig_index >= outputs_in_original_order.len() {
        return Err(format!("Invalid metadata host index: {}", metadata_host_orig_index));
    }

    if source_companion_orig_index >= outputs_in_original_order.len() {
        return Err(format!(
            "Recovered companion index {} is out of range.",
            source_companion_orig_index
        ));
    }

    if metadata_host_orig_index == source_companion_orig_index {
        return Err("Metadata host and source companion cannot be the same block.".to_string());
    }

    let high_output_key = view_value(&outputs_in_original_order[source_companion_orig_index], inverted_view)?;
    if high_output_key < BigUint::from(2u32) {
        return Err("High output key is too small to restore the pair with -2.".to_string());
    }

    // The selected pair was constructed to differ by 2 after encoding. Restore
    // the overwritten metadata host output in the same view used by that pass.
    let restored_low_old_output_key = &high_output_key - 2u32;
    let restored_low_old_output = view_value(&restored_low_old_output_key, inverted_view)?;
    outputs_in_original_order[metadata_host_orig_index] = restored_low_old_output;

    Ok((gap, source_companion_orig_index, metadata_host_orig_index))
}

fn decode_from_restored_outputs_in_original_order(
    restored_outputs_in_original_order: &[BigUint],
    gap: &BigUint,
    inverted_view: bool,
) -> Result<Vec<BigUint>, String> {
    let mut pairs: Vec<(usize, BigUint)> = Vec::with_capacity(restored_outputs_in_original_order.len());
    for (orig_index, output) in restored_outputs_in_original_order.iter().enumerate() {
        pairs.push((orig_index, view_value(output, inverted_view)?));
    }

    pairs.sort_by_key(|(_, y_key)| y_key.clone());

    let mut restored_in_original_order = vec![BigUint::zero(); restored_outputs_in_original_order.len()];

    for (sorted_rank, (orig_index, y_key)) in pairs.into_iter().enumerate() {
        // Reverse the rank-based shift in the same view used by encode.
        let x_key = y_key + BigUint::from((sorted_rank + 1) as u64) * gap;
        restored_in_original_order[orig_index] = view_value(&x_key, inverted_view)?;
    }

    Ok(restored_in_original_order)
}

fn write_plain_bits(path: &str, values: &[BigUint], width: u64) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    for value in values {
        writeln!(writer, "{}", to_bits(value, width))?;
    }

    Ok(())
}

fn write_plain_bits_auto(path: &str, values: &[BigUint]) -> Result<(), Box<dyn std::error::Error>> {
    let width = max_value_bit_width(values);
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    for value in values {
        writeln!(writer, "{}", to_bits(value, width))?;
    }

    Ok(())
}

fn read_plain_bits(path: &str, width: u64) -> Result<Vec<BigUint>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    let mut detected_width: Option<usize> = None;

    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if !trimmed.bytes().all(|b| b == b'0' || b == b'1') {
            return Err(format!("Invalid character in bit string. line={}", line_no + 1).into());
        }

        match detected_width {
            None => detected_width = Some(trimmed.len()),
            Some(current) if current != trimmed.len() => {
                return Err(format!(
                    "Inconsistent bit length. line={}, length={}, first_length={}",
                    line_no + 1,
                    trimmed.len(),
                    current
                ).into());
            }
            Some(_) => {}
        }

        if trimmed.len() < width as usize {
            return Err(format!(
                "Invalid bit length. line={}, length={}, minimum_expected={}",
                line_no + 1,
                trimmed.len(),
                width
            ).into());
        }

        values.push(from_bits(trimmed)?);
    }

    if let Some(detected) = detected_width {
        set_active_bit_width((detected as u64).max(width));
    }

    Ok(values)
}

fn write_smallest_two_outputs(outputs_in_original_order: &[BigUint]) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(SMALLEST_TWO_OUTPUTS_FILE)?;
    let mut writer = BufWriter::new(file);

    let mut sorted = outputs_in_original_order.to_vec();
    sorted.sort();

    let take_n = sorted.len().min(2);

    writeln!(writer, "DATE          : 2026-04-21")?;
    writeln!(writer, "BIT_WIDTH     : {}", active_bit_width())?;
    writeln!(writer, "COUNT         : {}", outputs_in_original_order.len())?;
    writeln!(writer, "FIELD         : smallest 2 encoded values in original block order output set")?;
    writeln!(writer)?;

    for (i, value) in sorted.iter().take(take_n).enumerate() {
        writeln!(writer, "RANK     : {}", i + 1)?;
        writeln!(writer, "OUT_DEC  : {}", value)?;
        writeln!(writer, "OUT_BITS : {}", to_bits(value, active_bit_width()))?;
        writeln!(writer, "{}", "-".repeat(80))?;
    }

    Ok(())
}

fn write_gap(
    source_left_sorted_pos: usize,
    source_right_sorted_pos: usize,
    source_left_orig_index: usize,
    source_right_orig_index: usize,
    raw_gap: &BigUint,
    gap: &BigUint,
    metadata_index_bits: u64,
    metadata_gap_bits: u64,
    encoded_outputs_in_sorted_order_before_embed: &[BigUint],
    encoded_outputs_in_original_order_after_embed: &[BigUint],
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(GAP_FILE)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "SOURCE_LEFT_SORTED_POS      : {}", source_left_sorted_pos)?;
    writeln!(writer, "SOURCE_RIGHT_SORTED_POS     : {}", source_right_sorted_pos)?;
    writeln!(writer, "SOURCE_LEFT_ORIG_INDEX      : {}", source_left_orig_index)?;
    writeln!(writer, "SOURCE_RIGHT_ORIG_INDEX     : {}", source_right_orig_index)?;
    writeln!(writer, "METADATA_HOST_ORIG_INDEX    : {}", source_left_orig_index)?;
    writeln!(writer, "SOURCE_COMPANION_ORIG_INDEX : {}", source_right_orig_index)?;
    writeln!(writer, "METADATA_INDEX_BITS         : {}", metadata_index_bits)?;
    writeln!(writer, "METADATA_GAP_BITS           : {}", metadata_gap_bits)?;
    writeln!(writer, "RAW_GAP_DEC                 : {}", raw_gap)?;
    writeln!(writer, "GAP_DEC                     : {}", gap)?;
    writeln!(writer)?;
    writeln!(
        writer,
        "SOURCE_LEFT_OLD_OUT_DEC     : {}",
        encoded_outputs_in_sorted_order_before_embed[source_left_sorted_pos]
    )?;
    writeln!(
        writer,
        "SOURCE_RIGHT_OLD_OUT_DEC    : {}",
        encoded_outputs_in_sorted_order_before_embed[source_right_sorted_pos]
    )?;
    writeln!(
        writer,
        "SOURCE_PAIR_OLD_DIFF_DEC    : {}",
        if encoded_outputs_in_sorted_order_before_embed[source_right_sorted_pos]
            >= encoded_outputs_in_sorted_order_before_embed[source_left_sorted_pos]
        {
            &encoded_outputs_in_sorted_order_before_embed[source_right_sorted_pos]
                - &encoded_outputs_in_sorted_order_before_embed[source_left_sorted_pos]
        } else {
            &encoded_outputs_in_sorted_order_before_embed[source_left_sorted_pos]
                - &encoded_outputs_in_sorted_order_before_embed[source_right_sorted_pos]
        }
    )?;
    writeln!(writer)?;
    writeln!(
        writer,
        "METADATA_HOST_NEW_OUT_DEC   : {}",
        encoded_outputs_in_original_order_after_embed[source_left_orig_index]
    )?;
    writeln!(
        writer,
        "SOURCE_COMPANION_NEW_OUT_DEC: {}",
        encoded_outputs_in_original_order_after_embed[source_right_orig_index]
    )?;

    Ok(())
}

fn write_space(
    sorted: &[SortedValue],
    encoded_records_in_sorted_order: &[EncodedRecord],
    outputs_in_original_order_after_embed: &[BigUint],
    restored_in_original_order: &[BigUint],
    gap: &BigUint,
    metadata_host_orig_index: usize,
    source_companion_orig_index: usize,
    metadata_index_bits: u64,
    metadata_gap_bits: u64,
    order_ok_before_embed: bool,
    reversible_ok: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(OUTPUT_FILE)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "DATE                        : 2026-04-21")?;
    writeln!(writer, "ORDER_OK_BEFORE_EMBED       : {}", order_ok_before_embed)?;
    writeln!(writer, "REVERSIBLE_OK               : {}", reversible_ok)?;
    writeln!(writer, "COUNT                       : {}", sorted.len())?;
    writeln!(writer, "BIT_WIDTH                   : {}", active_bit_width())?;
    writeln!(writer, "METADATA_INDEX_BITS         : {}", metadata_index_bits)?;
    writeln!(writer, "METADATA_GAP_BITS           : {}", metadata_gap_bits)?;
    writeln!(writer, "GAP                         : {}", gap)?;
    writeln!(writer, "METADATA_HOST_ORIG_INDEX    : {}", metadata_host_orig_index)?;
    writeln!(writer, "SOURCE_COMPANION_ORIG_INDEX : {}", source_companion_orig_index)?;
    writeln!(writer)?;

    let records_in_original_order = reorder_to_original_order(sorted, encoded_records_in_sorted_order);

    for i in 0..records_in_original_order.len() {
        let rec = &records_in_original_order[i];
        writeln!(writer, "ORIG_INDEX    : {}", i)?;
        writeln!(writer, "SORTED_RANK   : {}", rec.sorted_rank)?;
        writeln!(writer, "IN_DEC        : {}", rec.x)?;
        writeln!(writer, "OUT_DEC       : {}", outputs_in_original_order_after_embed[i])?;
        writeln!(writer, "BACK_DEC      : {}", restored_in_original_order[i])?;
        writeln!(writer, "SHIFT         : {}", rec.shift)?;
        if rec.is_metadata_host {
            writeln!(writer, "ROLE          : METADATA_HOST")?;
        } else if rec.is_source_companion {
            writeln!(writer, "ROLE          : SOURCE_COMPANION")?;
        } else {
            writeln!(writer, "ROLE          : NORMAL")?;
        }
        writeln!(writer, "IN_BITS       : {}", to_bits(&rec.x, active_bit_width()))?;
        writeln!(writer, "OUT_BITS      : {}", to_bits(&outputs_in_original_order_after_embed[i], active_bit_width()))?;
        writeln!(writer, "BACK_BITS     : {}", to_bits(&restored_in_original_order[i], active_bit_width()))?;
        writeln!(writer, "{}", "-".repeat(120))?;
    }

    Ok(())
}

fn repeated_output_name(output_file: &str, repeat: usize) -> String {
    let path = Path::new(output_file);
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(output_file);
    let extension = path.extension().and_then(|s| s.to_str());

    let file_name = match extension {
        Some(ext) => format!("{}.{}.{}", stem, repeat, ext),
        None => format!("{}.{}", stem, repeat),
    };

    match parent {
        Some(parent) => parent.join(file_name).to_string_lossy().into_owned(),
        None => file_name,
    }
}

fn repeat_count_from_path(path: &str) -> usize {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    stem.rsplit_once('.')
        .and_then(|(_, suffix)| suffix.parse::<usize>().ok())
        .filter(|repeat| *repeat > 0)
        .unwrap_or(1)
}

struct EncodePassResult {
    outputs: Vec<BigUint>,
}

// One encode pass. In repeat mode `encoded_output_file` is None so intermediate
// passes stay in memory and only the last successful repeat is written.
fn encode_values(
    values: &[BigUint],
    encoded_output_file: Option<&str>,
    persist_default_input_copy: bool,
    repeat: usize,
    write_debug_files: bool,
) -> Result<EncodePassResult, Box<dyn std::error::Error>> {
    if values.len() < 2 {
        return Err("Encode source must contain at least 2 blocks.".into());
    }
    set_active_bit_width(max_value_bit_width(values));
    let metadata_index_bits = bits_needed_for_count(values.len());
    let metadata_gap_bits = metadata_gap_bits_for_count(values.len())
        .map_err(|e| format!("Metadata layout error: {}", e))?;
    let inverted_view = use_inverted_view(repeat);

    validate_unique_values(values).map_err(|e| format!("Input validation error: {}", e))?;

    let sorted = sorted_values(values, inverted_view)
        .map_err(|e| format!("View error: {}", e))?;

    let (source_left_sorted_pos, source_right_sorted_pos, raw_gap, gap) =
        compute_gap_source(&sorted).map_err(|e| format!("Gap source error: {}", e))?;

    let shifts = build_shifts(sorted.len(), &gap);
    let (outputs_in_sorted_order_before_embed, encoded_records_in_sorted_order, order_ok_before_embed) =
        encode_sorted(&sorted, &shifts, source_left_sorted_pos, source_right_sorted_pos, inverted_view)
            .map_err(|e| format!("Encode error: {}", e))?;

    let mut outputs_in_original_order =
        reorder_to_original_order(&sorted, &outputs_in_sorted_order_before_embed);

    let metadata_host_orig_index = sorted[source_left_sorted_pos].orig_index;
    let source_companion_orig_index = sorted[source_right_sorted_pos].orig_index;

    embed_metadata_into_host(
        &mut outputs_in_original_order,
        metadata_host_orig_index,
        &gap,
        source_companion_orig_index,
        values.len(),
    ).map_err(|e| format!("Metadata embed error: {}", e))?;
    let mut outputs_for_decode_in_original_order = outputs_in_original_order.to_vec();
    let (recovered_gap, recovered_companion_pos, recovered_metadata_host_pos) =
        restore_selected_pair_before_decode(
            &mut outputs_for_decode_in_original_order,
            values.len(),
            inverted_view,
        )
            .map_err(|e| format!("Pre-decode restore error: {}", e))?;

    let restored_in_original_order =
        decode_from_restored_outputs_in_original_order(
            &outputs_for_decode_in_original_order,
            &recovered_gap,
            inverted_view,
        ).map_err(|e| format!("Decode view error: {}", e))?;

    let reversible_ok = restored_in_original_order
        .iter()
        .zip(values.iter())
        .all(|(a, b)| a == b);

    if write_debug_files {
        write_smallest_two_outputs(&outputs_in_original_order)?;
        write_gap(
            source_left_sorted_pos,
            source_right_sorted_pos,
            metadata_host_orig_index,
            source_companion_orig_index,
            &raw_gap,
            &gap,
            metadata_index_bits,
            metadata_gap_bits,
            &outputs_in_sorted_order_before_embed,
            &outputs_in_original_order,
        )?;
        write_space(
            &sorted,
            &encoded_records_in_sorted_order,
            &outputs_in_original_order,
            &restored_in_original_order,
            &recovered_gap,
            recovered_metadata_host_pos,
            recovered_companion_pos,
            metadata_index_bits,
            metadata_gap_bits,
            order_ok_before_embed,
            reversible_ok,
        )?;
    }
    if persist_default_input_copy {
        write_plain_bits(INPUT_BITS_FILE, values, bit_width())?;
    }
    if let Some(encoded_output_file) = encoded_output_file {
        write_plain_bits_auto(encoded_output_file, &outputs_in_original_order)?;
    }

    println!("Completed.");
    println!("Date                         : 2026-04-21");
    println!("Record count                 : {}", values.len());
    println!("Bit width                    : {}", active_bit_width());
    println!("Repeat                       : {}", repeat);
    println!("View                         : {}", if inverted_view { "inverted" } else { "normal" });
    println!("Restore delta                : 2");
    println!("Metadata index bits          : {}", metadata_index_bits);
    println!("Metadata GAP bits            : {}", metadata_gap_bits);
    println!("Metadata host original index : {}", recovered_metadata_host_pos);
    println!("Recovered companion index    : {}", recovered_companion_pos);
    println!("Metadata GAP                 : {}", summarize_biguint(&recovered_gap));
    println!("GAP/raw GAP                  : {}", summarize_gap_ratio(&gap, &raw_gap));
    println!("Order preserved before embed : {}", order_ok_before_embed);
    println!("Reversible                   : {}", reversible_ok);
    print_entropy("Input", values);
    print_entropy("Encoded", &outputs_in_original_order);
    print_proximity("Input", values);
    print_proximity("Encoded", &outputs_in_original_order);
    println!("space.txt                    : {}", OUTPUT_FILE);
    println!("smallest_two_outputs.txt     : {}", SMALLEST_TWO_OUTPUTS_FILE);
    println!("gap.txt                      : {}", GAP_FILE);
    if write_debug_files {
        println!("bits.txt                     : {}", BITS_FILE);
    }
    if persist_default_input_copy {
        println!("input.txt                    : {}", INPUT_BITS_FILE);
    }
    if let Some(encoded_output_file) = encoded_output_file {
        println!("encoded output               : {}", encoded_output_file);
    }

    Ok(EncodePassResult {
        outputs: outputs_in_original_order,
    })
}

fn run_encode_mode() -> Result<(), Box<dyn std::error::Error>> {
    clear_legacy_metadata_file()?;
    let values = generate_unique_values(config().count, bit_width());
    let result = encode_values(&values, Some(ENCODE_BITS_FILE), true, 1, true)?;
    if let Some(report) = collect_leading_ones_report(1, &result.outputs) {
        write_bits_report(&[report])?;
    }
    Ok(())
}

fn run_encode_file_mode(source_file: &str, output_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    clear_legacy_metadata_file()?;
    let values = read_plain_bits(source_file, bit_width())?;
    let result = encode_values(&values, Some(output_file), false, 1, true)?;
    if let Some(report) = collect_leading_ones_report(1, &result.outputs) {
        write_bits_report(&[report])?;
    }
    Ok(())
}

fn run_repeat_encode_mode(source_file: &str, output_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_repeat_encode_mode_with_limit(source_file, output_file, None)
}

// Repeat until a requested count is reached or the transform hits a safe
// stopping condition. Only the final successful repeat file is persisted.
fn persist_final_repeat_outputs(
    last_input_before_encode: &[BigUint],
    final_values: &[BigUint],
    final_file: &str,
    final_repeat: usize,
    leading_reports: &[LeadingOnesReport],
) -> Result<(), Box<dyn std::error::Error>> {
    let final_result = encode_values(
        last_input_before_encode,
        Some(final_file),
        false,
        final_repeat,
        true,
    )?;

    if final_result.outputs != final_values {
        return Err("Internal repeat finalization mismatch: recomputed final output differs.".into());
    }

    write_bits_report(leading_reports)?;
    println!("encoded output               : {}", final_file);
    Ok(())
}

// Repeat until a requested count is reached or the transform hits a safe
// stopping condition. Intermediate repeat passes stay in memory. Only the final
// successful output/debug files are persisted. metadata.txt is not used.
fn run_repeat_encode_mode_with_limit(
    source_file: &str,
    output_file: &str,
    max_repeats: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    clear_legacy_metadata_file()?;
    let mut values = read_plain_bits(source_file, bit_width())?;
    let mut repeat = 1usize;
    let mut last_completed_repeat = 0usize;
    let mut last_repeated_file = String::new();
    let mut last_input_before_encode: Option<Vec<BigUint>> = None;
    let mut leading_reports: Vec<LeadingOnesReport> = Vec::new();

    loop {
        if let Some(max_repeats) = max_repeats {
            if repeat > max_repeats {
                if last_completed_repeat > 0 {
                    persist_final_repeat_outputs(
                        last_input_before_encode
                            .as_ref()
                            .ok_or("Missing final repeat input during finalization.")?,
                        &values,
                        &last_repeated_file,
                        last_completed_repeat,
                        &leading_reports,
                    )?;
                } else {
                    write_bits_report(&leading_reports)?;
                }
                println!("Requested repeats completed  : {}", max_repeats);
                return Ok(());
            }
        }

        let repeated_file = repeated_output_name(output_file, repeat);
        let current_input = values.clone();
        match encode_values(&current_input, None, false, repeat, false) {
            Ok(result) => {
                println!("Repeat {} completed          : {}", repeat, repeated_file);
                if let Some(report) = collect_leading_ones_report(repeat, &result.outputs) {
                    leading_reports.push(report);
                }
                values = result.outputs;
                last_input_before_encode = Some(current_input);
                last_completed_repeat = repeat;
                last_repeated_file = repeated_file;
                repeat += 1;
            }
            Err(err) => {
                let message = err.to_string();
                let compression_limit_reached = message.contains("Gap source error: Safe GAP cannot be produced.")
                    || message.contains("Gap source error: Underflow risk at sorted rank")
                    || message.contains("Metadata embed error: Metadata self-sum overflow");

                if compression_limit_reached {
                    if last_completed_repeat > 0 {
                        persist_final_repeat_outputs(
                            last_input_before_encode
                                .as_ref()
                                .ok_or("Missing final repeat input during finalization.")?,
                            &values,
                            &last_repeated_file,
                            last_completed_repeat,
                                &leading_reports,
                        )?;
                    } else {
                            write_bits_report(&leading_reports)?;
                    }
                    println!("Repeat stopped before {}     : {}", repeat, message);
                    println!("Last completed repeat        : {}", last_completed_repeat);
                    return Ok(());
                }
                return Err(err);
            }
        }
    }
}

// Decode one repeat pass. The caller supplies the repeat number so the same
// normal/inverted view can be selected.
fn decode_values(outputs_in_original_order: &[BigUint], repeat: usize) -> Result<Vec<BigUint>, Box<dyn std::error::Error>> {
    if outputs_in_original_order.len() < 2 {
        return Err("Decode source must contain at least 2 blocks.".into());
    }
    set_active_bit_width(max_value_bit_width(outputs_in_original_order));
    let metadata_index_bits = bits_needed_for_count(outputs_in_original_order.len());
    let metadata_gap_bits = metadata_gap_bits_for_count(outputs_in_original_order.len())
        .map_err(|e| format!("Metadata layout error: {}", e))?;
    let inverted_view = use_inverted_view(repeat);
    let mut outputs_for_decode_in_original_order = outputs_in_original_order.to_vec();
    let (recovered_gap, recovered_companion_pos, metadata_host_orig_index) =
        restore_selected_pair_before_decode(
            &mut outputs_for_decode_in_original_order,
            outputs_in_original_order.len(),
            inverted_view,
        )
            .map_err(|e| format!("Pre-decode restore error: {}", e))?;

    let restored_in_original_order =
        decode_from_restored_outputs_in_original_order(
            &outputs_for_decode_in_original_order,
            &recovered_gap,
            inverted_view,
        ).map_err(|e| format!("Decode view error: {}", e))?;

    println!("Record count                 : {}", restored_in_original_order.len());
    println!("Bit width                    : {}", active_bit_width());
    println!("Repeat                       : {}", repeat);
    println!("View                         : {}", if inverted_view { "inverted" } else { "normal" });
    println!("Restore delta                : 2");
    println!("Metadata index bits          : {}", metadata_index_bits);
    println!("Metadata GAP bits            : {}", metadata_gap_bits);
    println!("Metadata host original index : {}", metadata_host_orig_index);
    println!("Recovered companion index    : {}", recovered_companion_pos);
    println!("Recovered GAP                : {}", summarize_biguint(&recovered_gap));
    print_entropy("Decode source", outputs_in_original_order);
    print_entropy("Restored", &restored_in_original_order);
    print_proximity("Decode source", outputs_in_original_order);
    print_proximity("Restored", &restored_in_original_order);

    Ok(restored_in_original_order)
}

fn run_decode_mode(source_file: &str, output_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    let repeat_count = repeat_count_from_path(source_file);
    let mut values = read_plain_bits(source_file, bit_width())?;

    // Repeat files are decoded backwards: .6, .5, ... .1.
    for repeat in (1..=repeat_count).rev() {
        println!("Decode pass                  : {}/{}", repeat_count - repeat + 1, repeat_count);
        values = decode_values(&values, repeat)?;
    }

    write_plain_bits_auto(output_file, &values)?;

    println!("Decode completed.");
    println!("Source file                  : {}", source_file);
    println!("Output file                  : {}", output_file);
    println!("Decode passes                : {}", repeat_count);

    Ok(())
}

fn parse_runtime_args(args: &[String]) -> Result<(Config, Vec<String>), String> {
    let mut bit_width = DEFAULT_BIT_WIDTH;
    let mut bit_width_explicit = false;
    let mut count = DEFAULT_COUNT;
    let mut positional = Vec::new();

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "-b" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value after -b.".to_string());
                }
                bit_width = args[i]
                    .parse::<u64>()
                    .map_err(|_| format!("Invalid bit width: {}", args[i]))?;
                bit_width_explicit = true;
                if bit_width == 0 {
                    return Err("Bit width must be at least 1.".to_string());
                }
            }
            "-c" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value after -c.".to_string());
                }
                count = args[i]
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid count: {}", args[i]))?;
                if count < 2 {
                    return Err("Count must be at least 2.".to_string());
                }
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    Ok((Config { bit_width, count, bit_width_explicit }, positional))
}

fn usage() -> &'static str {
    "Usage:\n  program [-b <bit_width>] [-c <count>]\n  program [-b <bit_width>] [-c <count>] -e <source_bits_file> <encoded_output_file>\n  program [-b <bit_width>] [-c <count>] -r <source_bits_file> <encoded_output_file>\n  program [-b <bit_width>] [-c <count>] -r <repeat_count> <source_bits_file> <encoded_output_file>\n  program [-b <bit_width>] [-c <count>] -d <source_bits_file> <decoded_output_file>\n\nExample:\n  program -b 256 -c 1000\n  program -b 256 -c 1000 -r input.txt output.txt\n\nNotes:\n  If -b is omitted for -e/-r/-d, bit width is detected from the first non-empty input line.\n  metadata.txt is not used in the self-sum metadata experiment; self-sum overflow control is temporarily disabled.\n  bits.txt is written only after the final encode/repeat result."
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let (mut runtime_config, positional) = parse_runtime_args(&args)
        .map_err(|err| format!("{}\n\n{}", err, usage()))?;

    if !runtime_config.bit_width_explicit {
        let source_for_detection = if positional.len() == 3 && (positional[0] == "-e" || positional[0] == "-r" || positional[0] == "-d") {
            Some(positional[1].as_str())
        } else if positional.len() == 4 && positional[0] == "-r" {
            Some(positional[2].as_str())
        } else {
            None
        };

        if let Some(source_file) = source_for_detection {
            runtime_config.bit_width = detect_bit_width_from_file(source_file)?;
        }
    }

    if CONFIG.set(runtime_config).is_err() {
        return Err("Runtime config was already initialized.".into());
    }
    set_active_bit_width(config().bit_width);

    if positional.len() == 3 && positional[0] == "-e" {
        return run_encode_file_mode(&positional[1], &positional[2]);
    }

    if positional.len() == 3 && positional[0] == "-r" {
        return run_repeat_encode_mode(&positional[1], &positional[2]);
    }

    if positional.len() == 4 && positional[0] == "-r" {
        let max_repeats = positional[1]
            .parse::<usize>()
            .map_err(|_| format!("Invalid repeat count: {}", positional[1]))?;
        if max_repeats == 0 {
            return Err("Repeat count must be at least 1.".into());
        }
        return run_repeat_encode_mode_with_limit(&positional[2], &positional[3], Some(max_repeats));
    }

    if positional.len() == 3 && positional[0] == "-d" {
        return run_decode_mode(&positional[1], &positional[2]);
    }

    if !positional.is_empty() {
        return Err(usage().into());
    }

    run_encode_mode()
}
