// Repositioning of Information Combined Within Data Space
// © 2026 Mesut Erturhan / GITHUB PIYXU
// Licensed under the GNU General Public License v3.0.

// Code implementation and enhancement assisted by ChatGPT.
// Core principles developed in collaboration with Gemini.
// https://github.com/piyxu/RICWDS/

// ==========================================================
// CORE IDEA
// ----------------------------------------------------------
// We transform 256-bit numbers using a single reversible GAP.
//
// RULES OF THIS VERSION
// ----------------------------------------------------------
// 1) We do NOT start from the smallest number.
//    The smallest value stays unchanged.
//    Encoding starts from the 2nd smallest value.
//
// 2) GAP is selected with:
//      GAP = min(active_min_gap - 2, start_value / active_count)
//
//    where:
//      active_count   = number of values from START_INDEX to end
//      start_value    = sorted[START_INDEX]
//      active_min_gap = minimum adjacent gap inside active region
//
// 3) Encode on ascending order:
//
//      for active rank r = 0,1,2,...:
//          shift = (r+1) * GAP
//          y = x - shift
//
// 4) Decode:
//
//      x = y + shift
//
// EXTRA FILES
// ----------------------------------------------------------
// input.txt  : plain input bit strings, one per line
// encode.txt : plain encoded bit strings, one per line
//
// DECODE MODE
// ----------------------------------------------------------
// cargo run -- -d <source_bits_file> <decoded_output_file>
// or
// program -d <source_bits_file> <decoded_output_file>
//
// Decode mode reads GAP from metadata.txt and decodes the bit
// strings in the given source file into the given output file.
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
const START_INDEX: usize = 1; // 0 => en küçükten başla, 1 => 2. en küçükten başla
const OUTPUT_FILE: &str = "space.txt";
const METADATA_FILE: &str = "metadata.txt";
const SMALLEST_TWO_OUTPUTS_FILE: &str = "smallest_two_outputs.txt";
const GAP_FILE: &str = "gap.txt";
const INPUT_BITS_FILE: &str = "input.txt";
const ENCODE_BITS_FILE: &str = "encode.txt";

#[derive(Clone, Debug)]
struct EncodedRecord {
    orig_index: usize,
    x: BigUint,
    y: BigUint,
    shift: BigUint,
    active_rank: Option<usize>,
}

#[derive(Clone, Debug)]
struct DecodedRecord {
    orig_index: usize,
    y: BigUint,
    x: BigUint,
    shift: BigUint,
    active_rank: Option<usize>,
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
        .ok_or_else(|| format!("Geçersiz bit dizisi: {}", s.trim()))
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
// Compute reversible GAP using min_gap - 2 on active region
// ----------------------------------------------------------
fn compute_metadata(sorted: &[BigUint], start_index: usize) -> Result<(BigUint, BigUint, Vec<BigUint>), String> {
    if sorted.len() < 2 {
        return Err("En az 2 sayı gerekli.".to_string());
    }

    if start_index >= sorted.len() {
        return Err(format!(
            "Geçersiz START_INDEX. start_index={}, len={}",
            start_index,
            sorted.len()
        ));
    }

    let active_count = sorted.len() - start_index;
    if active_count == 0 {
        return Err("Aktif bölge boş.".to_string());
    }

    let all_diffs = compute_differences(sorted);

    if active_count == 1 {
        return Err("Aktif bölgede en az 2 sayı gerekli.".to_string());
    }

    let active_diffs = all_diffs[start_index..].to_vec();
    let active_min_gap = active_diffs
        .iter()
        .min()
        .cloned()
        .ok_or_else(|| "Aktif min_gap hesaplanamadı.".to_string())?;

    let two = BigUint::from(2u32);
    if active_min_gap <= two {
        return Err(format!(
            "Güvenli GAP üretilemedi. aktif min_gap={} <= 2",
            active_min_gap
        ));
    }

    let gap_by_order = &active_min_gap - 2u32;
    let start_value = sorted[start_index].clone();
    let gap_by_value = &start_value / BigUint::from(active_count as u64);

    let gap = if gap_by_order < gap_by_value {
        gap_by_order
    } else {
        gap_by_value
    };

    if gap.is_zero() {
        return Err(format!(
            "Metadata GAP sıfır oldu. start_value={}, active_count={}",
            start_value, active_count
        ));
    }

    Ok((active_min_gap, gap, all_diffs))
}

fn build_gap_shifts(total_count: usize, start_index: usize, gap: &BigUint) -> Vec<BigUint> {
    (0..total_count)
        .map(|i| {
            if i < start_index {
                BigUint::zero()
            } else {
                let active_rank = i - start_index;
                BigUint::from((active_rank + 1) as u64) * gap
            }
        })
        .collect()
}

// ----------------------------------------------------------
// Encode values using ascending order and active-region shifts
// ----------------------------------------------------------
fn encode(
    values: &[BigUint],
    shifts: &[BigUint],
) -> Result<(Vec<BigUint>, bool, Vec<EncodedRecord>), String> {
    let mut sorted_pairs: Vec<(usize, BigUint)> = values.iter().cloned().enumerate().collect();
    sorted_pairs.sort_by_key(|(_, v)| v.clone());

    let mut asc_encoded: Vec<EncodedRecord> = Vec::with_capacity(values.len());

    for (sorted_rank, ((orig_index, x), s)) in sorted_pairs.into_iter().zip(shifts.iter()).enumerate() {
        if x < *s {
            return Err(format!(
                "Encode başarısız: x < shift. x={}, shift={}",
                x, s
            ));
        }

        let y = &x - s;
        let active_rank = if s.is_zero() { None } else { Some(sorted_rank - START_INDEX) };

        asc_encoded.push(EncodedRecord {
            orig_index,
            x,
            y,
            shift: s.clone(),
            active_rank,
        });
    }

    let order_ok = asc_encoded.windows(2).all(|w| w[0].y < w[1].y);

    let mut by_input_order = asc_encoded.clone();
    by_input_order.sort_by_key(|r| r.orig_index);
    let outputs = by_input_order.iter().map(|r| r.y.clone()).collect();

    Ok((outputs, order_ok, by_input_order))
}

// ----------------------------------------------------------
// Decode values by reconstructing active-region shifts
// ----------------------------------------------------------
fn decode(outputs: &[BigUint], shifts: &[BigUint]) -> (Vec<BigUint>, Vec<DecodedRecord>) {
    let mut sorted_pairs: Vec<(usize, BigUint)> = outputs.iter().cloned().enumerate().collect();
    sorted_pairs.sort_by_key(|(_, v)| v.clone());

    let mut asc_decoded: Vec<DecodedRecord> = Vec::with_capacity(outputs.len());

    for (sorted_rank, ((orig_index, y), s)) in sorted_pairs.into_iter().zip(shifts.iter()).enumerate() {
        let x = &y + s;
        let active_rank = if s.is_zero() { None } else { Some(sorted_rank - START_INDEX) };

        asc_decoded.push(DecodedRecord {
            orig_index,
            y,
            x,
            shift: s.clone(),
            active_rank,
        });
    }

    let mut by_input_order = asc_decoded.clone();
    by_input_order.sort_by_key(|r| r.orig_index);
    let restored = by_input_order.iter().map(|r| r.x.clone()).collect();

    (restored, by_input_order)
}

fn write_metadata(
    metadata: &BigUint,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(METADATA_FILE)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "{}", to_bits(metadata, BIT_WIDTH))?;

    Ok(())
}

fn write_plain_bits(path: &str, values: &[BigUint]) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    for value in values {
        writeln!(writer, "{}", to_bits(value, BIT_WIDTH))?;
    }

    Ok(())
}

fn read_plain_bits(path: &str) -> Result<Vec<BigUint>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();

    for (line_no, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.len() != BIT_WIDTH as usize {
            return Err(format!(
                "Geçersiz bit uzunluğu. satır={}, uzunluk={}, beklenen={}",
                line_no + 1,
                trimmed.len(),
                BIT_WIDTH
            )
            .into());
        }

        if !trimmed.bytes().all(|b| b == b'0' || b == b'1') {
            return Err(format!(
                "Geçersiz karakter içeren bit dizisi. satır={}",
                line_no + 1
            )
            .into());
        }

        values.push(from_bits(trimmed)?);
    }

    Ok(values)
}

fn read_metadata_bits(path: &str) -> Result<BigUint, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return Ok(from_bits(trimmed)?);
    }

    Err("metadata.txt boş.".into())
}

fn write_smallest_two_outputs(outputs: &[BigUint]) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(SMALLEST_TWO_OUTPUTS_FILE)?;
    let mut writer = BufWriter::new(file);

    let mut sorted = outputs.to_vec();
    sorted.sort();

    let take_n = sorted.len().min(2);

    writeln!(writer, "DATE          : 2026-04-21")?;
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
    gap: &BigUint,
    active_min_gap: &BigUint,
    order_ok: bool,
    reversible_ok: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(OUTPUT_FILE)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "DATE            : 2026-04-21")?;
    writeln!(writer, "ORDER_OK        : {}", order_ok)?;
    writeln!(writer, "REVERSIBLE_OK   : {}", reversible_ok)?;
    writeln!(writer, "COUNT           : {}", values.len())?;
    writeln!(writer, "BIT_WIDTH       : {}", BIT_WIDTH)?;
    writeln!(writer, "START_INDEX     : {}", START_INDEX)?;
    writeln!(writer, "ACTIVE_MIN_GAP  : {}", active_min_gap)?;
    writeln!(writer, "GAP             : {}", gap)?;
    writeln!(writer)?;

    for i in 0..values.len() {
        writeln!(writer, "INDEX       : {}", i)?;
        writeln!(writer, "IN_DEC      : {}", values[i])?;
        writeln!(writer, "OUT_DEC     : {}", outputs[i])?;
        if order_ok {
            writeln!(writer, "BACK_DEC    : {}", restored[i])?;
        } else {
            writeln!(writer, "BACK_DEC    : FAILED_ORDER_CHECK")?;
        }

        writeln!(writer, "IN_BITS     : {}", to_bits(&values[i], BIT_WIDTH))?;
        writeln!(writer, "OUT_BITS    : {}", to_bits(&outputs[i], BIT_WIDTH))?;
        if order_ok {
            writeln!(writer, "BACK_BITS   : {}", to_bits(&restored[i], BIT_WIDTH))?;
        } else {
            writeln!(writer, "BACK_BITS   : FAILED_ORDER_CHECK")?;
        }

        writeln!(writer, "SHIFT       : {}", encoded_records[i].shift)?;
        match encoded_records[i].active_rank {
            Some(rank) => writeln!(writer, "ACTIVE_RANK : {}", rank)?,
            None => writeln!(writer, "ACTIVE_RANK : NONE")?,
        }
        writeln!(writer, "ORIG_X      : {}", encoded_records[i].x)?;
        writeln!(writer, "ENC_Y       : {}", encoded_records[i].y)?;
        writeln!(writer, "{}", "-".repeat(120))?;
    }

    Ok(())
}

fn run_encode_mode() -> Result<(), Box<dyn std::error::Error>> {
    let values = generate_unique_values(COUNT, BIT_WIDTH);

    let asc_sorted = sorted_values(&values);
    let (active_min_gap, metadata_gap, _diffs) = compute_metadata(&asc_sorted, START_INDEX)?;
    let shifts = build_gap_shifts(COUNT, START_INDEX, &metadata_gap);

    let (outputs, order_ok, encoded_records) =
        encode(&values, &shifts).map_err(|e| format!("Encode hatası: {}", e))?;

    let (restored, reversible_ok) = if order_ok {
        let (restored, decoded_records) = decode(&outputs, &shifts);
        let ok = restored == values;
        let _decoded_records_guard = decoded_records
            .iter()
            .map(|r| (&r.y, &r.shift, &r.active_rank))
            .collect::<Vec<_>>();
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

    write_metadata(&metadata_gap)?;
    write_smallest_two_outputs(&outputs)?;
    write_gap(&outputs)?;
    write_plain_bits(INPUT_BITS_FILE, &values)?;
    write_plain_bits(ENCODE_BITS_FILE, &outputs)?;
    write_space(
        &values,
        &outputs,
        &encoded_records,
        &restored,
        &metadata_gap,
        &active_min_gap,
        order_ok,
        reversible_ok,
    )?;

    println!("Completed.");
    println!("Date                      : 2026-04-21");
    println!("Record count              : {}", COUNT);
    println!("Bit width                 : {}", BIT_WIDTH);
    println!("Start index               : {}", START_INDEX);
    println!("Active min gap            : {}", active_min_gap);
    println!("Metadata GAP              : {}", metadata_gap);
    println!("Order preserved           : {}", order_ok);
    println!("Reversible                : {}", reversible_ok);
    println!("Changed records           : {}", changed_count);
    println!("Unchanged records         : {}", same_count);
    println!("metadata.txt              : {}", METADATA_FILE);
    println!("space.txt                 : {}", OUTPUT_FILE);
    println!("smallest_two_outputs.txt  : {}", SMALLEST_TWO_OUTPUTS_FILE);
    println!("gap.txt                   : {}", GAP_FILE);
    println!("input.txt                 : {}", INPUT_BITS_FILE);
    println!("encode.txt                : {}", ENCODE_BITS_FILE);

    Ok(())
}

fn run_decode_mode(source_file: &str, output_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(METADATA_FILE).exists() {
        return Err(format!(
            "Decode için {} gerekli. Önce normal encode çalıştırılmalı.",
            METADATA_FILE
        )
        .into());
    }

    let outputs = read_plain_bits(source_file)?;
    if outputs.is_empty() {
        return Err("Decode kaynağı boş.".into());
    }

    let metadata_gap = read_metadata_bits(METADATA_FILE)?;
    if metadata_gap.is_zero() {
        return Err("metadata.txt içindeki GAP sıfır.".into());
    }

    let shifts = build_gap_shifts(outputs.len(), START_INDEX, &metadata_gap);
    let (restored, decoded_records) = decode(&outputs, &shifts);
    let _sanity = decoded_records
        .iter()
        .map(|r| (&r.orig_index, &r.y, &r.x, &r.shift, &r.active_rank))
        .collect::<Vec<_>>();

    write_plain_bits(output_file, &restored)?;

    println!("Decode completed.");
    println!("Source file               : {}", source_file);
    println!("Output file               : {}", output_file);
    println!("Record count              : {}", restored.len());
    println!("Bit width                 : {}", BIT_WIDTH);
    println!("Start index               : {}", START_INDEX);
    println!("Metadata GAP              : {}", metadata_gap);

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() == 4 && args[1] == "-d" {
        return run_decode_mode(&args[2], &args[3]);
    }

    if args.len() != 1 {
        return Err(
            "Kullanım:\n  program\n  program -d <kaynak_bit_dosyasi> <decode_cikti_dosyasi>"
                .into(),
        );
    }

    run_encode_mode()
}

