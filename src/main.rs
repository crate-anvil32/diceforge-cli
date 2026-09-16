use std::env;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use diceforge::{parse, Roll, RollResult};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: diceforge <notation> [<notation> ...]");
        eprintln!("example: diceforge 3d6+2");
        return ExitCode::FAILURE;
    }

    let mut rng = SplitMix64::seeded_from_clock();
    let mut had_error = false;

    for arg in &args {
        match parse(arg) {
            Ok(roll) => {
                let result = roll.evaluate(|sides| rng.roll_die(sides));
                print_result(arg, &roll, &result);
            }
            Err(err) => {
                eprintln!("{arg}: {err}");
                had_error = true;
            }
        }
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_result(source: &str, roll: &Roll, result: &RollResult) {
    let rolls: Vec<String> = result.dice.iter().map(u32::to_string).collect();
    let mut line = format!("{source}: [{}]", rolls.join(", "));

    if roll.keep.is_some() {
        let kept: Vec<String> = result.kept.iter().map(u32::to_string).collect();
        line.push_str(&format!(" keep [{}]", kept.join(", ")));
    }
    if roll.modifier != 0 {
        line.push_str(&format!(" {:+}", roll.modifier));
    }
    line.push_str(&format!(" = {}", result.total));

    println!("{line}");
}

/// A small, dependency-free PRNG (SplitMix64) used only by the CLI to
/// supply real randomness. The library itself stays pure; see
/// `Roll::evaluate` in `src/lib.rs`.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn seeded_from_clock() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15);
        SplitMix64 { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    /// Returns a value in `1..=sides`. Uses a modulo reduction, which is
    /// good enough for die sizes that appear in dice notation.
    fn roll_die(&mut self, sides: u32) -> u32 {
        (self.next_u64() % sides as u64) as u32 + 1
    }
}
