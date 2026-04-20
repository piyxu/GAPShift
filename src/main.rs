// Repositioning of Information Combined Within Data Space
// © 2026 Mesut Erturhan / GITHUB PIYXU
// Licensed under the GNU General Public License v3.0.

// Code implementation and enhancement assisted by ChatGPT.
// Core principles developed in collaboration with Gemini.
// https://github.com/piyxu/RICWDS/

// ==========================================================
// CORE IDEA
// ----------------------------------------------------------
// We transform a set of 256-bit numbers by subtracting
// deterministic shifts derived from a single metadata value D.
//
// Encode:
//   y = x - shift(i)
//   shift(i) = (i+1) * D
//
// Decode:
//   x = y + shift(i)
//
// Key property:
// - Order must be preserved after encoding (order_ok)
// - If order is preserved, decode works WITHOUT storing shifts
//   explicitly, only by recomputing them using D.
//
// SAFETY CONSTRAINTS
// ----------------------------------------------------------
// 1) No overlap (ordering preserved):
//      D < min_gap
//
// 2) No underflow (stay >= 0):
//      COUNT * D <= min_value
//
// Final safe selection:
//      D = min((min_gap // 2) - 2, min_value // COUNT)
//
// ==========================================================

use num_bigint::{BigUint, RandBigInt};
use num_traits::Zero;
use rand::rngs::OsRng;
use std::cmp::Reverse;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Write};

const BIT_WIDTH: u64 = 256;
const COUNT: usize = 10_000;
const OUTPUT_FILE: &str = "space.txt";
const METADATA_FILE: &str = "metadata.txt";
const SMALLEST_TWO_OUTPUTS_FILE: &str = "smallest_two_outputs.txt";
const GAP_FILE: &str = "gap.txt";

#[derive(Clone, Debug)]
struct EncodedRecord {
    orig_index: usize,
    x: BigUint,
    y: BigUint,
    shift: BigUint,
}

#[derive(Clone, Debug)]
struct DecodedRecord {
    orig_index: usize,
    y: BigUint,
    x: BigUint,
    shift: BigUint,
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

fn sorted_values(values: &[BigUint]) -> Vec<BigUint> {
    let mut v = values.to_vec();
    v.sort();
    v
}

fn compute_differences(sorted: &[BigUint]) -> Vec<BigUint> {
    let mut diffs = Vec::with_capacity(sorted.len().saturating_sub(1));
    for i in 1..sorted.len() {
        diffs.push(&sorted[i] - &sorted[i - 1]);
    }
    diffs
}

// ----------------------------------------------------------
// Compute metadata (D) using gap + zero-bound constraints
// ----------------------------------------------------------
fn compute_metadata(sorted: &[BigUint], count: usize) -> Result<(BigUint, BigUint, Vec<BigUint>), String> {
    if sorted.len() < 2 {
        return Err("En az 2 sayı gerekli.".to_string());
    }

    let diffs = compute_differences(sorted);
    let min_gap = diffs
        .iter()
        .min()
        .cloned()
        .ok_or_else(|| "Fark hesaplanamadı.".to_string())?;

    let four = BigUint::from(4u32);
    if min_gap <= four {
        return Err(format!(
            "Güvenli metadata üretilemedi. min_gap={} <= 4",
            min_gap
        ));
    }

    let d_gap = (&min_gap / 2u32) - 2u32;
    let min_value = sorted[0].clone();
    let d_value_limit = &min_value / BigUint::from(count as u64);

    let d = if d_gap < d_value_limit {
        d_gap
    } else {
        d_value_limit
    };

    if d.is_zero() {
        return Err(format!(
            "Metadata sıfır oldu. min_value={}, count={}",
            min_value, count
        ));
    }

    Ok((min_gap, d, diffs))
}

fn build_desc_shifts(count: usize, d: &BigUint) -> Vec<BigUint> {
    (0..count)
        .map(|i| BigUint::from((i + 1) as u64) * d)
        .collect()
}

// ----------------------------------------------------------
// Encode values using descending order and deterministic shifts
// ----------------------------------------------------------
fn encode(values: &[BigUint], shifts: &[BigUint]) -> Result<(Vec<BigUint>, bool, Vec<EncodedRecord>), String> {
    let mut sorted_pairs: Vec<(usize, BigUint)> = values.iter().cloned().enumerate().collect();
    sorted_pairs.sort_by_key(|(_, v)| Reverse(v.clone()));

    let mut desc_encoded: Vec<EncodedRecord> = Vec::with_capacity(values.len());

    for ((orig_index, x), s) in sorted_pairs.into_iter().zip(shifts.iter()) {
        if x < *s {
            return Err(format!(
                "Encode başarısız: x < shift. x={}, shift={}",
                x, s
            ));
        }

        let y = &x - s;

        desc_encoded.push(EncodedRecord {
            orig_index,
            x,
            y,
            shift: s.clone(),
        });
    }

    let order_ok = desc_encoded.windows(2).all(|w| w[0].y > w[1].y);

    let mut by_input_order = desc_encoded.clone();
    by_input_order.sort_by_key(|r| r.orig_index);
    let outputs = by_input_order.iter().map(|r| r.y.clone()).collect();

    Ok((outputs, order_ok, by_input_order))
}

// ----------------------------------------------------------
// Decode values by reconstructing shifts using the same D
// ----------------------------------------------------------
fn decode(outputs: &[BigUint], shifts: &[BigUint]) -> (Vec<BigUint>, Vec<DecodedRecord>) {
    let mut sorted_pairs: Vec<(usize, BigUint)> = outputs.iter().cloned().enumerate().collect();
    sorted_pairs.sort_by_key(|(_, v)| Reverse(v.clone()));

    let mut desc_decoded: Vec<DecodedRecord> = Vec::with_capacity(outputs.len());

    for ((orig_index, y), s) in sorted_pairs.into_iter().zip(shifts.iter()) {
        let x = &y + s;

        desc_decoded.push(DecodedRecord {
            orig_index,
            y,
            x,
            shift: s.clone(),
        });
    }

    let mut by_input_order = desc_decoded.clone();
    by_input_order.sort_by_key(|r| r.orig_index);
    let restored = by_input_order.iter().map(|r| r.x.clone()).collect();

    (restored, by_input_order)
}

fn write_metadata(
    _min_gap: &BigUint,
    metadata: &BigUint,
    _diffs: &[BigUint],
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(METADATA_FILE)?;
    let mut writer = BufWriter::new(file);

    let bits = to_bits(metadata, BIT_WIDTH);
    writeln!(writer, "{}", bits)?;

    Ok(())
}

fn write_smallest_two_outputs(outputs: &[BigUint]) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(SMALLEST_TWO_OUTPUTS_FILE)?;
    let mut writer = BufWriter::new(file);

    let mut sorted = outputs.to_vec();
    sorted.sort();

    let take_n = sorted.len().min(2);

    writeln!(writer, "DATE          : 2026-04-20")?;
    writeln!(writer, "BIT_WIDTH     : {}", BIT_WIDTH)?;
    writeln!(writer, "COUNT         : {}", outputs.len())?;
    writeln!(writer, "FIELD         : smallest 2 full OUT values")?;
    writeln!(writer)?;

    for (i, value) in sorted.iter().take(take_n).enumerate() {
        writeln!(writer, "RANK     : {}", i + 1)?;
        writeln!(writer, "OUT_DEC  : {}", value)?;
        writeln!(writer, "OUT_BITS : {}", to_bits(value, BIT_WIDTH))?;
        writeln!(writer, "{}", "-".repeat(80))?;
    }

    Ok(())
}

// ----------------------------------------------------------
// Write smallest two outputs and their gap for analysis
// ----------------------------------------------------------
fn write_gap(outputs: &[BigUint]) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(GAP_FILE)?;
    let mut writer = BufWriter::new(file);

    let mut sorted = outputs.to_vec();
    sorted.sort();

    if sorted.len() < 2 {
        return Err("Yeterli veri yok.".into());
    }

    let first = &sorted[0];
    let second = &sorted[1];
    let gap = second - first;

    writeln!(writer, "FIRST_OUT_DEC  : {}", first)?;
    writeln!(writer, "SECOND_OUT_DEC : {}", second)?;
    writeln!(writer, "GAP_DEC        : {}", gap)?;
    writeln!(writer)?;
    writeln!(writer, "FIRST_OUT_BITS  : {}", to_bits(first, BIT_WIDTH))?;
    writeln!(writer, "SECOND_OUT_BITS : {}", to_bits(second, BIT_WIDTH))?;

    Ok(())
}

fn write_space(
    values: &[BigUint],
    outputs: &[BigUint],
    encoded_records: &[EncodedRecord],
    restored: &[BigUint],
    d: &BigUint,
    order_ok: bool,
    reversible_ok: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(OUTPUT_FILE)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "DATE          : 2026-04-20")?;
    writeln!(writer, "ORDER_OK      : {}", order_ok)?;
    writeln!(writer, "REVERSIBLE_OK : {}", reversible_ok)?;
    writeln!(writer, "COUNT         : {}", values.len())?;
    writeln!(writer, "BIT_WIDTH     : {}", BIT_WIDTH)?;
    writeln!(writer, "D             : {}", d)?;
    writeln!(writer)?;

    for i in 0..values.len() {
        writeln!(writer, "INDEX : {}", i)?;
        writeln!(writer, "IN_DEC   : {}", values[i])?;
        writeln!(writer, "OUT_DEC  : {}", outputs[i])?;
        if order_ok {
            writeln!(writer, "BACK_DEC : {}", restored[i])?;
        } else {
            writeln!(writer, "BACK_DEC : FAILED_ORDER_CHECK")?;
        }

        writeln!(writer, "IN_BITS  : {}", to_bits(&values[i], BIT_WIDTH))?;
        writeln!(writer, "OUT_BITS : {}", to_bits(&outputs[i], BIT_WIDTH))?;
        if order_ok {
            writeln!(writer, "BACK_BITS: {}", to_bits(&restored[i], BIT_WIDTH))?;
        } else {
            writeln!(writer, "BACK_BITS: FAILED_ORDER_CHECK")?;
        }

        writeln!(writer, "SHIFT    : {}", encoded_records[i].shift)?;
        writeln!(writer, "{}", "-".repeat(120))?;
    }

    Ok(())
}

// ----------------------------------------------------------
// MAIN PIPELINE
// ----------------------------------------------------------
// 1) Generate random values
// 2) Compute metadata (D)
// 3) Build shifts
// 4) Encode
// 5) Decode (if order preserved)
// 6) Write outputs
// ----------------------------------------------------------
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let values = generate_unique_values(COUNT, BIT_WIDTH);

    let asc_sorted = sorted_values(&values);
    let (min_gap, metadata_d, diffs) = compute_metadata(&asc_sorted, COUNT)?;
    let shifts = build_desc_shifts(COUNT, &metadata_d);

    let (outputs, order_ok, encoded_records) = encode(&values, &shifts)
        .map_err(|e| format!("Encode hatası: {}", e))?;

    let (restored, reversible_ok) = if order_ok {
        let (restored, _) = decode(&outputs, &shifts);
        let ok = restored == values;
        (restored, ok)
    } else {
        (vec![BigUint::zero(); values.len()], false)
    };

    let changed_count = values
        .iter()
        .zip(outputs.iter())
        .filter(|(a, b)| *a != *b)
        .count();

    let same_count = COUNT - changed_count;

    write_metadata(&min_gap, &metadata_d, &diffs)?;
    write_smallest_two_outputs(&outputs)?;
    write_gap(&outputs)?;
    write_space(
        &values,
        &outputs,
        &encoded_records,
        &restored,
        &metadata_d,
        order_ok,
        reversible_ok,
    )?;

    println!("Tamamlandı.");
    println!("Tarih                     : 2026-04-20");
    println!("Kayıt sayısı              : {}", COUNT);
    println!("Bit width                 : {}", BIT_WIDTH);
    println!("Min gap                   : {}", min_gap);
    println!("Metadata D                : {}", metadata_d);
    println!("Sıra korunuyor mu         : {}", order_ok);
    println!("Tersinirlik               : {}", reversible_ok);
    println!("Değişen kayıt             : {}", changed_count);
    println!("Aynı kalan kayıt          : {}", same_count);
    println!("metadata.txt              : {}", METADATA_FILE);
    println!("space.txt                 : {}", OUTPUT_FILE);
    println!("smallest_two_outputs.txt  : {}", SMALLEST_TWO_OUTPUTS_FILE);
    println!("gap.txt                   : {}", GAP_FILE);

    Ok(())
}
