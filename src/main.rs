// GAPShift: A Reversible Data Transformation
// © 2026 Mesut Erturhan / GITHUB PIYXU
// Licensed under the GNU General Public License v3.0.

// Code implementation and enhancement assisted by ChatGPT.
// Core principles developed in collaboration with Gemini.
// https://github.com/piyxu/GAPShift/
//GAPShift Version Update Version 0.1

// ==========================================================
// CORE IDEA
// ----------------------------------------------------------
// GAPShift transforms 256-bit numbers with a single reversible
// GAP derived from the closest adjacent pair in sorted order.
//
// This version keeps the original block order in output files,
// embeds metadata into one selected block, and performs decode
// using only:
//
//   1) encode.txt
//   2) metadata.txt
//
// No input/index file is required for decode.
//
// MAIN RULES OF THIS VERSION
// ----------------------------------------------------------
// 1) All values are sorted in ascending order only for GAP
//    selection and linear rank-based encoding.
//
// 2) The closest adjacent sorted pair is selected:
//
//      raw_gap = min(sorted[i+1] - sorted[i])
//
//    This pair becomes the source pair.
//
// 3) The operational GAP is:
//
//      GAP = raw_gap - 2
//
// 4) Encoding is linear by sorted rank:
//
//      shift(rank) = (rank + 1) * GAP
//      y = x - shift(rank)
//
//    where rank starts from 0 in ascending sorted order.
//
// 5) One block of the selected source pair is used as the
//    metadata host block.
//
// 6) The metadata host block stores:
//    - GAP
//    - companion block original position
//
//    These are packed into the same 256-bit block.
//
// 7) metadata.txt stores only the original position of the
//    metadata host block.
//
//    Its width is dynamic:
//
//      position_bits = ceil(log2(COUNT))
//
// 8) input.txt and encode.txt are written in ORIGINAL block
//    order, not sorted order.
//
// 9) Before decode starts, the selected source pair is restored
//    to its old encoded form:
//
//      old_low_output = high_output - 2
//
//    This works because the selected source pair was encoded so
//    that its old encoded difference is exactly 2.
//
// 10) After restoring the selected pair, the decoder rebuilds
//     the rank order in memory by sorting the restored encoded
//     values.
//
// 11) Normal decode then continues with:
//
//      x = y + shift(rank)
//
//     using the recovered GAP.
//
// EXTRA FILES
// ----------------------------------------------------------
// input.txt
//   Original values as plain 256-bit binary strings, one per line.
//
// encode.txt
//   Encoded values as plain 256-bit binary strings, one per line,
//   written in original block order.
//
// metadata.txt
//   Stores only the original position of the metadata host block.
//   Bit width is dynamic and depends on COUNT.
//
// gap.txt
//   Stores technical GAP information, source-pair positions,
//   raw_gap, GAP, and related encoded values.
//
// space.txt
//   Stores detailed transformation records for debugging and
//   inspection.
//
// smallest_two_outputs.txt
//   Stores the two smallest encoded outputs.
//
// DECODE MODE
// ----------------------------------------------------------
// cargo run -- -d <source_bits_file> <decoded_output_file>
// or
// program -d <source_bits_file> <decoded_output_file>
//
// Decode flow:
// 1) read metadata host position from metadata.txt
// 2) read GAP + companion position from the metadata host block
// 3) restore the selected source pair
// 4) sort restored encoded outputs in memory
// 5) perform normal decode
// 6) write decoded values in original block order
// ==========================================================

use num_bigint::{BigUint, RandBigInt};
use num_traits::Zero;
use rand::rngs::OsRng;
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

const BIT_WIDTH: u64 = 256;
const COUNT: usize = 10_000;
const OUTPUT_FILE: &str = "space.txt";
const METADATA_FILE: &str = "metadata.txt";
const SMALLEST_TWO_OUTPUTS_FILE: &str = "smallest_two_outputs.txt";
const GAP_FILE: &str = "gap.txt";
const INPUT_BITS_FILE: &str = "input.txt";
const ENCODE_BITS_FILE: &str = "encode.txt";

#[derive(Clone, Debug)]
struct SortedValue {
    sorted_rank: usize,
    orig_index: usize,
    x: BigUint,
}

#[derive(Clone, Debug)]
struct EncodedRecord {
    sorted_rank: usize,
    orig_index: usize,
    x: BigUint,
    shift: BigUint,
    is_metadata_host: bool,
    is_source_companion: bool,
}

fn bits_needed_for_count(count: usize) -> u64 {
    if count <= 1 {
        1
    } else {
        (usize::BITS - (count - 1).leading_zeros()) as u64
    }
}

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

fn from_bits(s: &str) -> Result<BigUint, String> {
    BigUint::parse_bytes(s.trim().as_bytes(), 2)
        .ok_or_else(|| format!("Invalid bit string: {}", s.trim()))
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

fn sorted_values(values: &[BigUint]) -> Vec<SortedValue> {
    let mut pairs: Vec<(usize, BigUint)> = values.iter().cloned().enumerate().collect();
    pairs.sort_by_key(|(_, v)| v.clone());

    pairs
        .into_iter()
        .enumerate()
        .map(|(sorted_rank, (orig_index, x))| SortedValue {
            sorted_rank,
            orig_index,
            x,
        })
        .collect()
}

fn compute_gap_source(sorted: &[SortedValue]) -> Result<(usize, usize, BigUint, BigUint), String> {
    if sorted.len() < 2 {
        return Err("At least 2 numbers are required.".to_string());
    }

    let mut best_left = 0usize;
    let mut best_right = 1usize;
    let mut best_raw_gap = &sorted[1].x - &sorted[0].x;

    for i in 1..sorted.len() - 1 {
        let diff = &sorted[i + 1].x - &sorted[i].x;
        if diff < best_raw_gap {
            best_raw_gap = diff;
            best_left = i;
            best_right = i + 1;
        }
    }

    let two = BigUint::from(2u32);
    if best_raw_gap <= two {
        return Err(format!("Safe GAP cannot be produced. raw_gap={} <= 2", best_raw_gap));
    }

    let gap = &best_raw_gap - 2u32;

    for (i, sv) in sorted.iter().enumerate() {
        let shift = BigUint::from((i + 1) as u64) * &gap;
        if sv.x < shift {
            return Err(format!(
                "Underflow risk at sorted rank {}: value {} is less than shift {}",
                i, sv.x, shift
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
) -> Result<(Vec<BigUint>, Vec<EncodedRecord>, bool), String> {
    let mut outputs = Vec::with_capacity(sorted.len());
    let mut records = Vec::with_capacity(sorted.len());

    for (sv, shift) in sorted.iter().zip(shifts.iter()) {
        if sv.x < *shift {
            return Err(format!("Encode failed: x < shift. x={}, shift={}", sv.x, shift));
        }

        let y = &sv.x - shift;
        outputs.push(y.clone());
        records.push(EncodedRecord {
            sorted_rank: sv.sorted_rank,
            orig_index: sv.orig_index,
            x: sv.x.clone(),
            shift: shift.clone(),
            is_metadata_host: sv.sorted_rank == source_left_sorted_pos,
            is_source_companion: sv.sorted_rank == source_right_sorted_pos,
        });
    }

    let order_ok = outputs.windows(2).all(|w| w[0] < w[1]);
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

fn pack_metadata_block(gap: &BigUint, companion_orig_index: usize, pos_bits: u64) -> Result<BigUint, String> {
    let companion = BigUint::from(companion_orig_index as u64);
    let max_pos = BigUint::from(1u32) << pos_bits;

    if companion >= max_pos {
        return Err(format!(
            "Companion position {} does not fit in {} bits.",
            companion_orig_index, pos_bits
        ));
    }

    let packed = (gap << pos_bits) | companion;

    if packed.bits() > BIT_WIDTH {
        return Err(format!(
            "Packed metadata block exceeds {} bits. packed_bits={}",
            BIT_WIDTH,
            packed.bits()
        ));
    }

    Ok(packed)
}

fn unpack_metadata_block(packed: &BigUint, pos_bits: u64) -> Result<(BigUint, usize), String> {
    let mask = (BigUint::from(1u32) << pos_bits) - 1u32;
    let companion = packed & &mask;
    let gap = packed >> pos_bits;

    if gap.is_zero() {
        return Err("Recovered GAP is zero.".to_string());
    }

    let digits = companion.to_u64_digits();
    let companion_pos = if digits.is_empty() { 0 } else { digits[0] as usize };

    Ok((gap, companion_pos))
}

fn embed_metadata_into_host(
    outputs_in_original_order: &mut [BigUint],
    metadata_host_orig_index: usize,
    gap: &BigUint,
    source_companion_orig_index: usize,
    pos_bits: u64,
) -> Result<(), String> {
    let packed = pack_metadata_block(gap, source_companion_orig_index, pos_bits)?;
    outputs_in_original_order[metadata_host_orig_index] = packed;
    Ok(())
}

fn restore_selected_pair_before_decode(
    outputs_in_original_order: &mut [BigUint],
    metadata_host_orig_index: usize,
    pos_bits: u64,
) -> Result<(BigUint, usize), String> {
    if metadata_host_orig_index >= outputs_in_original_order.len() {
        return Err(format!("Invalid metadata host index: {}", metadata_host_orig_index));
    }

    let packed = outputs_in_original_order[metadata_host_orig_index].clone();
    let (gap, source_companion_orig_index) = unpack_metadata_block(&packed, pos_bits)?;

    if source_companion_orig_index >= outputs_in_original_order.len() {
        return Err(format!(
            "Recovered companion index {} is out of range.",
            source_companion_orig_index
        ));
    }

    if metadata_host_orig_index == source_companion_orig_index {
        return Err("Metadata host and source companion cannot be the same block.".to_string());
    }

    let high_output = outputs_in_original_order[source_companion_orig_index].clone();
    if high_output < BigUint::from(2u32) {
        return Err("High output is too small to restore the pair with -2.".to_string());
    }

    let restored_low_old_output = &high_output - 2u32;
    outputs_in_original_order[metadata_host_orig_index] = restored_low_old_output;

    Ok((gap, source_companion_orig_index))
}

fn decode_from_restored_outputs_in_original_order(
    restored_outputs_in_original_order: &[BigUint],
    gap: &BigUint,
) -> Vec<BigUint> {
    let mut pairs: Vec<(usize, BigUint)> = restored_outputs_in_original_order
        .iter()
        .cloned()
        .enumerate()
        .collect();

    pairs.sort_by_key(|(_, y)| y.clone());

    let mut restored_in_original_order = vec![BigUint::zero(); restored_outputs_in_original_order.len()];

    for (sorted_rank, (orig_index, y)) in pairs.into_iter().enumerate() {
        let x = y + BigUint::from((sorted_rank + 1) as u64) * gap;
        restored_in_original_order[orig_index] = x;
    }

    restored_in_original_order
}

fn write_metadata_host_pos(pos: usize, pos_bits: u64) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(METADATA_FILE)?;
    let mut writer = BufWriter::new(file);
    let pos_big = BigUint::from(pos as u64);
    writeln!(writer, "{}", to_bits(&pos_big, pos_bits))?;
    Ok(())
}

fn read_metadata_host_pos(path: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = from_bits(trimmed)?;
        let digits = value.to_u64_digits();
        let pos = if digits.is_empty() { 0 } else { digits[0] as usize };
        return Ok(pos);
    }

    Err("metadata.txt is empty.".into())
}

fn write_plain_bits(path: &str, values: &[BigUint], width: u64) -> Result<(), Box<dyn std::error::Error>> {
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

    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.len() != width as usize {
            return Err(format!(
                "Invalid bit length. line={}, length={}, expected={}",
                line_no + 1,
                trimmed.len(),
                width
            ).into());
        }

        if !trimmed.bytes().all(|b| b == b'0' || b == b'1') {
            return Err(format!("Invalid character in bit string. line={}", line_no + 1).into());
        }

        values.push(from_bits(trimmed)?);
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
    writeln!(writer, "BIT_WIDTH     : {}", BIT_WIDTH)?;
    writeln!(writer, "COUNT         : {}", outputs_in_original_order.len())?;
    writeln!(writer, "FIELD         : smallest 2 encoded values in original block order output set")?;
    writeln!(writer)?;

    for (i, value) in sorted.iter().take(take_n).enumerate() {
        writeln!(writer, "RANK     : {}", i + 1)?;
        writeln!(writer, "OUT_DEC  : {}", value)?;
        writeln!(writer, "OUT_BITS : {}", to_bits(value, BIT_WIDTH))?;
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
    pos_bits: u64,
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
    writeln!(writer, "POSITION_BITS               : {}", pos_bits)?;
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
        &encoded_outputs_in_sorted_order_before_embed[source_right_sorted_pos]
            - &encoded_outputs_in_sorted_order_before_embed[source_left_sorted_pos]
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
    pos_bits: u64,
    order_ok_before_embed: bool,
    reversible_ok: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(OUTPUT_FILE)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "DATE                        : 2026-04-21")?;
    writeln!(writer, "ORDER_OK_BEFORE_EMBED       : {}", order_ok_before_embed)?;
    writeln!(writer, "REVERSIBLE_OK               : {}", reversible_ok)?;
    writeln!(writer, "COUNT                       : {}", sorted.len())?;
    writeln!(writer, "BIT_WIDTH                   : {}", BIT_WIDTH)?;
    writeln!(writer, "POSITION_BITS               : {}", pos_bits)?;
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
        writeln!(writer, "IN_BITS       : {}", to_bits(&rec.x, BIT_WIDTH))?;
        writeln!(writer, "OUT_BITS      : {}", to_bits(&outputs_in_original_order_after_embed[i], BIT_WIDTH))?;
        writeln!(writer, "BACK_BITS     : {}", to_bits(&restored_in_original_order[i], BIT_WIDTH))?;
        writeln!(writer, "{}", "-".repeat(120))?;
    }

    Ok(())
}

fn run_encode_mode() -> Result<(), Box<dyn std::error::Error>> {
    let values = generate_unique_values(COUNT, BIT_WIDTH);
    let sorted = sorted_values(&values);
    let pos_bits = bits_needed_for_count(COUNT);

    let (source_left_sorted_pos, source_right_sorted_pos, raw_gap, gap) =
        compute_gap_source(&sorted).map_err(|e| format!("Gap source error: {}", e))?;

    let shifts = build_shifts(sorted.len(), &gap);
    let (outputs_in_sorted_order_before_embed, encoded_records_in_sorted_order, order_ok_before_embed) =
        encode_sorted(&sorted, &shifts, source_left_sorted_pos, source_right_sorted_pos)
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
        pos_bits,
    ).map_err(|e| format!("Metadata embed error: {}", e))?;

    let mut outputs_for_decode_in_original_order = outputs_in_original_order.clone();
    let (recovered_gap, recovered_companion_pos) = restore_selected_pair_before_decode(
        &mut outputs_for_decode_in_original_order,
        metadata_host_orig_index,
        pos_bits,
    ).map_err(|e| format!("Pre-decode restore error: {}", e))?;

    let restored_in_original_order =
        decode_from_restored_outputs_in_original_order(&outputs_for_decode_in_original_order, &recovered_gap);

    let reversible_ok = restored_in_original_order
        .iter()
        .zip(values.iter())
        .all(|(a, b)| a == b);

    write_metadata_host_pos(metadata_host_orig_index, pos_bits)?;
    write_smallest_two_outputs(&outputs_in_original_order)?;
    write_gap(
        source_left_sorted_pos,
        source_right_sorted_pos,
        metadata_host_orig_index,
        source_companion_orig_index,
        &raw_gap,
        &gap,
        pos_bits,
        &outputs_in_sorted_order_before_embed,
        &outputs_in_original_order,
    )?;
    write_plain_bits(INPUT_BITS_FILE, &values, BIT_WIDTH)?;
    write_plain_bits(ENCODE_BITS_FILE, &outputs_in_original_order, BIT_WIDTH)?;
    write_space(
        &sorted,
        &encoded_records_in_sorted_order,
        &outputs_in_original_order,
        &restored_in_original_order,
        &recovered_gap,
        metadata_host_orig_index,
        recovered_companion_pos,
        pos_bits,
        order_ok_before_embed,
        reversible_ok,
    )?;

    println!("Completed.");
    println!("Date                         : 2026-04-21");
    println!("Record count                 : {}", COUNT);
    println!("Bit width                    : {}", BIT_WIDTH);
    println!("Position bits                : {}", pos_bits);
    println!("Metadata host original index : {}", metadata_host_orig_index);
    println!("Recovered companion index    : {}", recovered_companion_pos);
    println!("Metadata GAP                 : {}", recovered_gap);
    println!("Order preserved before embed : {}", order_ok_before_embed);
    println!("Reversible                   : {}", reversible_ok);
    println!("metadata.txt                 : {}", METADATA_FILE);
    println!("space.txt                    : {}", OUTPUT_FILE);
    println!("smallest_two_outputs.txt     : {}", SMALLEST_TWO_OUTPUTS_FILE);
    println!("gap.txt                      : {}", GAP_FILE);
    println!("input.txt                    : {}", INPUT_BITS_FILE);
    println!("encode.txt                   : {}", ENCODE_BITS_FILE);

    Ok(())
}

fn run_decode_mode(source_file: &str, output_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(METADATA_FILE).exists() {
        return Err(format!("{} is required for decode. Run normal encode first.", METADATA_FILE).into());
    }

    let outputs_in_original_order = read_plain_bits(source_file, BIT_WIDTH)?;
    if outputs_in_original_order.len() < 2 {
        return Err("Decode source must contain at least 2 blocks.".into());
    }

    let pos_bits = bits_needed_for_count(outputs_in_original_order.len());
    let metadata_host_orig_index = read_metadata_host_pos(METADATA_FILE)?;
    if metadata_host_orig_index >= outputs_in_original_order.len() {
        return Err(format!(
            "Metadata host index {} is out of range for {} blocks.",
            metadata_host_orig_index,
            outputs_in_original_order.len()
        ).into());
    }

    let mut outputs_for_decode_in_original_order = outputs_in_original_order.clone();
    let (recovered_gap, recovered_companion_pos) = restore_selected_pair_before_decode(
        &mut outputs_for_decode_in_original_order,
        metadata_host_orig_index,
        pos_bits,
    ).map_err(|e| format!("Pre-decode restore error: {}", e))?;

    let restored_in_original_order =
        decode_from_restored_outputs_in_original_order(&outputs_for_decode_in_original_order, &recovered_gap);

    write_plain_bits(output_file, &restored_in_original_order, BIT_WIDTH)?;

    println!("Decode completed.");
    println!("Source file                  : {}", source_file);
    println!("Output file                  : {}", output_file);
    println!("Record count                 : {}", restored_in_original_order.len());
    println!("Bit width                    : {}", BIT_WIDTH);
    println!("Position bits                : {}", pos_bits);
    println!("Metadata host original index : {}", metadata_host_orig_index);
    println!("Recovered companion index    : {}", recovered_companion_pos);
    println!("Recovered GAP                : {}", recovered_gap);

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() == 4 && args[1] == "-d" {
        return run_decode_mode(&args[2], &args[3]);
    }

    if args.len() != 1 {
        return Err(
            "Usage:\n  program\n  program -d <source_bits_file> <decoded_output_file>"
                .into(),
        );
    }

    run_encode_mode()
}
