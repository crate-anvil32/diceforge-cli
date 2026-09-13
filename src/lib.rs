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
/// then add a flat `modifier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roll {
    pub count: u32,
    pub sides: u32,
    pub modifier: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    MissingSides,
    InvalidCount(String),
    InvalidSides(String),
    InvalidModifier(String),
    ZeroSides,
    ZeroCount,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty dice expression"),
            ParseError::MissingSides => write!(f, "missing 'd<sides>' after count"),
            ParseError::InvalidCount(s) => write!(f, "invalid die count: {s:?}"),
            ParseError::InvalidSides(s) => write!(f, "invalid side count: {s:?}"),
            ParseError::InvalidModifier(s) => write!(f, "invalid modifier: {s:?}"),
            ParseError::ZeroSides => write!(f, "a die must have at least one side"),
            ParseError::ZeroCount => write!(f, "must roll at least one die"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses standard dice notation, e.g. "3d6", "1d20+5", "2d4-1".
///
/// The count before `d` may be omitted (defaults to 1), so "d20" is the
/// same as "1d20". Whitespace around the whole expression is ignored,
/// but not in the middle of it.
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
    let (sides_str, modifier_str) = split_modifier(rest);

    let sides: u32 = sides_str
        .parse()
        .map_err(|_| ParseError::InvalidSides(sides_str.to_string()))?;
    if sides == 0 {
        return Err(ParseError::ZeroSides);
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
        modifier,
    })
}

/// Splits "20+5" into ("20", "+5") or "20" into ("20", "").
/// The sign stays attached to the modifier half so it parses as `i32`.
fn split_modifier(rest: &str) -> (&str, &str) {
    match rest.find(['+', '-']) {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    }
}

/// The outcome of evaluating a [`Roll`]: each individual die result plus
/// the final total after the modifier is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollResult {
    pub dice: Vec<u32>,
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
        let sum: i32 = dice.iter().map(|&d| d as i32).sum();
        RollResult {
            dice,
            modifier: self.modifier,
            total: sum + self.modifier,
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
                modifier: 3
            }
        );
        assert_eq!(
            parse("2d4-1").unwrap(),
            Roll {
                count: 2,
                sides: 4,
                modifier: -1
            }
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
}
