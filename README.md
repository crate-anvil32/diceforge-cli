# diceforge

A small Rust library for parsing and rolling tabletop dice notation
("3d6+2", "1d20", "2d4-1"), plus a thin CLI on top of it.

## Why

Every tabletop game and dice-based generator ends up needing the same
small parser: turn a string like `3d6+2` into "roll three six-sided
dice and add two", then actually roll it. It's easy to write badly -
usually by tangling the parsing, the randomness, and the printing
together so none of it can be tested without mocking a global RNG.

This crate keeps those pieces separate:

- `parse(&str) -> Result<Roll, ParseError>` turns notation into a
  `Roll` struct. Pure string-in, struct-out.
- `Roll::evaluate(next_die)` rolls the dice, but takes its source of
  randomness as a parameter instead of calling into a global RNG. Feed
  it a real PRNG in production, or a fixed sequence in a test - the
  function itself has no side effects either way.

Only the CLI binary touches actual randomness (a small seeded PRNG,
std-only, no external crate).

## Usage

As a library:

```rust
use diceforge::parse;

let roll = parse("3d6+2").unwrap();

// Deterministic "roll" for a test: always return 4.
let result = roll.evaluate(|_sides| 4);
assert_eq!(result.dice, vec![4, 4, 4]);
assert_eq!(result.total, 4 + 4 + 4 + 2);
```

A real caller supplies a real die source instead of a constant closure
- see `src/main.rs` for the PRNG the CLI uses.

As a CLI:

```
$ cargo run -- 3d6+2
3d6+2: [4, 1, 6] +2 = 13

$ cargo run -- 1d20 2d4-1
1d20: [17] = 17
2d4-1: [3, 2] -1 = 4
```

## Notation supported

- `NdM` - roll N dice with M sides (`3d6`)
- `dM` - shorthand for `1dM` (`d20`)
- `NdM+K` / `NdM-K` - flat modifier added or subtracted (`2d4+3`)

## Status

Early skeleton. Parsing and evaluation work and are unit tested;
nothing more elaborate (keep-highest, exploding dice, dice pools) is
implemented yet.

## License

MIT, see `LICENSE`.
