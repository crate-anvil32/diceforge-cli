//! Parsing and evaluation for tabletop dice notation such as `3d6+2`.
//!
//! Every public function here is pure: [`parse`] does string-to-struct
//! conversion with no side effects, and [`Roll::evaluate`] takes its
//! source of randomness as a parameter instead of reaching for a global
//! RNG. That makes both trivial to unit test with fixed inputs - see the
//! tests module below for examples of rolling "randomly" in a fully
//! deterministic way.

use std::fmt;

/// A parsed dice expression: roll `count` dice with `sides` faces each,
/// optionally keep only the highest/lowest of them, then add a flat
/// `modifier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roll {
    pub count: u32,
    pub sides: u32,
    pub keep: Option<Keep>,
    pub modifier: i32,
}

/// Which dice survive after a roll, for notation like `4d6kh3` (keep the
/// highest 3 of 4) or `2d20kl1` (keep the lowest 1 of 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    Highest(u32),
    Lowest(u32),
}

impl Keep {
    fn count(self) -> u32 {
        match self {
            Keep::Highest(n) | Keep::Lowest(n) => n,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    MissingSides,
    InvalidCount(String),
    InvalidSides(String),
    InvalidKeep(String),
    InvalidModifier(String),
    ZeroSides,
    ZeroCount,
    ZeroKeep,
    KeepExceedsCount { keep: u32, count: u32 },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty dice expression"),
            ParseError::MissingSides => write!(f, "missing 'd<sides>' after count"),
            ParseError::InvalidCount(s) => write!(f, "invalid die count: {s:?}"),
            ParseError::InvalidSides(s) => write!(f, "invalid side count: {s:?}"),
            ParseError::InvalidKeep(s) => write!(f, "invalid keep modifier: {s:?}"),
            ParseError::InvalidModifier(s) => write!(f, "invalid modifier: {s:?}"),
            ParseError::ZeroSides => write!(f, "a die must have at least one side"),
            ParseError::ZeroCount => write!(f, "must roll at least one die"),
            ParseError::ZeroKeep => write!(f, "must keep at least one die"),
            ParseError::KeepExceedsCount { keep, count } => {
                write!(f, "cannot keep {keep} dice out of only {count} rolled")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses standard dice notation, e.g. "3d6", "1d20+5", "2d4-1",
/// "4d6kh3", "2d20kl1+1".
///
/// The count before `d` may be omitted (defaults to 1), so "d20" is the
/// same as "1d20". A keep modifier (`kh<n>` or `kl<n>`) may follow the
/// side count to keep only the highest or lowest `n` of the rolled dice.
/// Whitespace around the whole expression is ignored, but not in the
/// middle of it.
pub fn parse(input: &str) -> Result<Roll, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    let d_pos = trimmed
        .to_ascii_lowercase()
        .find('d')
        .ok_or(ParseError::MissingSides)?;

    let count_str = &trimmed[..d_pos];
    let count: u32 = if count_str.is_empty() {
        1
    } else {
        count_str
            .parse()
            .map_err(|_| ParseError::InvalidCount(count_str.to_string()))?
    };
    if count == 0 {
        return Err(ParseError::ZeroCount);
    }

    let rest = &trimmed[d_pos + 1..];
    let digit_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let sides_str = &rest[..digit_end];
    let after_sides = &rest[digit_end..];

    let sides: u32 = sides_str
        .parse()
        .map_err(|_| ParseError::InvalidSides(sides_str.to_string()))?;
    if sides == 0 {
        return Err(ParseError::ZeroSides);
    }

    let (keep, modifier_str) = parse_keep(after_sides)?;
    if let Some(k) = keep {
        let n = k.count();
        if n == 0 {
            return Err(ParseError::ZeroKeep);
        }
        if n > count {
            return Err(ParseError::KeepExceedsCount { keep: n, count });
        }
    }

    let modifier: i32 = match modifier_str {
        "" => 0,
        m => m
            .parse()
            .map_err(|_| ParseError::InvalidModifier(m.to_string()))?,
    };

    Ok(Roll {
        count,
        sides,
        keep,
        modifier,
    })
}

/// Parses an optional `kh<n>` / `kl<n>` prefix off `s`, returning the
/// keep rule (if any) and whatever's left over (typically a modifier, or
/// nothing). A leading byte other than `k`/`K` means there's no keep
/// clause at all, so `s` is handed back untouched.
fn parse_keep(s: &str) -> Result<(Option<Keep>, &str), ParseError> {
    if !matches!(s.as_bytes().first(), Some(b'k') | Some(b'K')) {
        return Ok((None, s));
    }

    let kind = s.as_bytes().get(1).map(u8::to_ascii_lowercase);
    let rest = &s[s.len().min(2)..];
    let digit_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let num_str = &rest[..digit_end];
    if num_str.is_empty() {
        return Err(ParseError::InvalidKeep(s.to_string()));
    }
    let n: u32 = num_str
        .parse()
        .map_err(|_| ParseError::InvalidKeep(s.to_string()))?;

    let keep = match kind {
        Some(b'h') => Keep::Highest(n),
        Some(b'l') => Keep::Lowest(n),
        _ => return Err(ParseError::InvalidKeep(s.to_string())),
    };
    Ok((Some(keep), &rest[digit_end..]))
}

/// The outcome of evaluating a [`Roll`]: every individual die rolled, the
/// subset actually kept (all of them, unless a keep rule dropped some),
/// and the final total after the modifier is applied to the kept dice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollResult {
    pub dice: Vec<u32>,
    pub kept: Vec<u32>,
    pub modifier: i32,
    pub total: i32,
}

impl Roll {
    /// Evaluates the roll using `next_die` as the source of randomness.
    ///
    /// `next_die(sides)` must return a value in `1..=sides`. Passing a
    /// closure over a fixed sequence (rather than a real RNG) is how
    /// callers keep this function pure and deterministic for tests; the
    /// CLI is the only place that wires in actual randomness.
    pub fn evaluate<F>(&self, mut next_die: F) -> RollResult
    where
        F: FnMut(u32) -> u32,
    {
        let dice: Vec<u32> = (0..self.count).map(|_| next_die(self.sides)).collect();
        let kept = self.apply_keep(&dice);
        let sum: i32 = kept.iter().map(|&d| d as i32).sum();
        RollResult {
            dice,
            kept,
            modifier: self.modifier,
            total: sum + self.modifier,
        }
    }

    /// Applies this roll's keep rule (if any) to a set of rolled dice,
    /// returning the subset that counts toward the total. Ties are
    /// broken by rolled order, which only matters for which *copy* of a
    /// repeated value is reported kept, not for the total itself.
    fn apply_keep(&self, dice: &[u32]) -> Vec<u32> {
        match self.keep {
            None => dice.to_vec(),
            Some(Keep::Highest(n)) => {
                let mut sorted = dice.to_vec();
                sorted.sort_unstable_by(|a, b| b.cmp(a));
                sorted.truncate(n as usize);
                sorted
            }
            Some(Keep::Lowest(n)) => {
                let mut sorted = dice.to_vec();
                sorted.sort_unstable();
                sorted.truncate(n as usize);
                sorted
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_notation() {
        assert_eq!(
            parse("3d6").unwrap(),
            Roll {
                count: 3,
                sides: 6,
                keep: None,
                modifier: 0
            }
        );
    }

    #[test]
    fn parses_missing_count_as_one() {
        assert_eq!(
            parse("d20").unwrap(),
            Roll {
                count: 1,
                sides: 20,
                keep: None,
                modifier: 0
            }
        );
    }

    #[test]
    fn parses_positive_and_negative_modifiers() {
        assert_eq!(
            parse("2d4+3").unwrap(),
            Roll {
                count: 2,
                sides: 4,
                keep: None,
                modifier: 3
            }
        );
        assert_eq!(
            parse("2d4-1").unwrap(),
            Roll {
                count: 2,
                sides: 4,
                keep: None,
                modifier: -1
            }
        );
    }

    #[test]
    fn parses_keep_highest_and_lowest() {
        assert_eq!(
            parse("4d6kh3").unwrap(),
            Roll {
                count: 4,
                sides: 6,
                keep: Some(Keep::Highest(3)),
                modifier: 0
            }
        );
        assert_eq!(
            parse("2d20kl1").unwrap(),
            Roll {
                count: 2,
                sides: 20,
                keep: Some(Keep::Lowest(1)),
                modifier: 0
            }
        );
    }

    #[test]
    fn parses_keep_is_case_insensitive_and_allows_a_trailing_modifier() {
        assert_eq!(
            parse("4d6KH3+2").unwrap(),
            Roll {
                count: 4,
                sides: 6,
                keep: Some(Keep::Highest(3)),
                modifier: 2
            }
        );
    }

    #[test]
    fn rejects_malformed_keep_clauses() {
        assert!(matches!(parse("4d6k"), Err(ParseError::InvalidKeep(_))));
        assert!(matches!(parse("4d6kx3"), Err(ParseError::InvalidKeep(_))));
        assert!(matches!(parse("4d6kh"), Err(ParseError::InvalidKeep(_))));
    }

    #[test]
    fn rejects_keep_count_of_zero() {
        assert_eq!(parse("4d6kh0"), Err(ParseError::ZeroKeep));
    }

    #[test]
    fn rejects_keep_count_larger_than_dice_rolled() {
        assert_eq!(
            parse("4d6kh5"),
            Err(ParseError::KeepExceedsCount { keep: 5, count: 4 })
        );
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(parse("  1d20+5  ").unwrap(), parse("1d20+5").unwrap());
    }

    #[test]
    fn rejects_empty_input() {
        assert_eq!(parse(""), Err(ParseError::Empty));
        assert_eq!(parse("   "), Err(ParseError::Empty));
    }

    #[test]
    fn rejects_missing_d() {
        assert_eq!(parse("20"), Err(ParseError::MissingSides));
    }

    #[test]
    fn rejects_zero_count_and_sides() {
        assert_eq!(parse("0d6"), Err(ParseError::ZeroCount));
        assert_eq!(parse("1d0"), Err(ParseError::ZeroSides));
    }

    #[test]
    fn rejects_garbage_fields() {
        assert!(matches!(parse("xd6"), Err(ParseError::InvalidCount(_))));
        assert!(matches!(parse("1dx"), Err(ParseError::InvalidSides(_))));
        assert!(matches!(
            parse("1d6+x"),
            Err(ParseError::InvalidModifier(_))
        ));
    }

    #[test]
    fn evaluate_is_deterministic_given_a_fixed_source() {
        let roll = parse("3d6+2").unwrap();
        let mut sequence = vec![1, 6, 4].into_iter();
        let result = roll.evaluate(|_sides| sequence.next().unwrap());

        assert_eq!(result.dice, vec![1, 6, 4]);
        assert_eq!(result.kept, vec![1, 6, 4]);
        assert_eq!(result.modifier, 2);
        assert_eq!(result.total, 1 + 6 + 4 + 2);
    }

    #[test]
    fn evaluate_respects_requested_die_count() {
        let roll = parse("5d1").unwrap();
        let result = roll.evaluate(|sides| sides);
        assert_eq!(result.dice.len(), 5);
        assert_eq!(result.dice, vec![1, 1, 1, 1, 1]);
    }

    #[test]
    fn evaluate_keeps_only_the_highest_dice() {
        let roll = parse("4d6kh3").unwrap();
        let mut sequence = vec![5, 2, 6, 1].into_iter();
        let result = roll.evaluate(|_sides| sequence.next().unwrap());

        assert_eq!(result.dice, vec![5, 2, 6, 1]);
        assert_eq!(result.kept, vec![6, 5, 2]);
        assert_eq!(result.total, 6 + 5 + 2);
    }

    #[test]
    fn evaluate_keeps_only_the_lowest_dice() {
        let roll = parse("4d6kl2+1").unwrap();
        let mut sequence = vec![5, 2, 6, 1].into_iter();
        let result = roll.evaluate(|_sides| sequence.next().unwrap());

        assert_eq!(result.dice, vec![5, 2, 6, 1]);
        assert_eq!(result.kept, vec![1, 2]);
        assert_eq!(result.total, 1 + 2 + 1);
    }
}
