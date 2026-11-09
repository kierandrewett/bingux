# Calculator

`bingux-calculator` is the Bingux calculator: Basic, Scientific, Programmer and
Convert on one exact engine. The window is `apps/calculator`; the engine is
`crates/calc` (`bingux-calc`), which has no UI and is tested on its own.

## Use it

Type on the keyboard or press the keys. The result shows as you type, even for
an unfinished expression (`5 +` shows 5, `2 * (3 + 4` shows 14). Enter finishes
the calculation and adds it to the history.

| Key              | What it does                                          |
| ---------------- | ----------------------------------------------------- |
| Enter            | Finish the calculation                                |
| Escape           | Clear the entry; with nothing typed, clear the answer |
| Backspace        | Delete the last character                             |
| Ctrl+1 to Ctrl+4 | Basic, Scientific, Programmer, Convert                |
| Ctrl+H           | Show or hide the history                              |
| Ctrl+Shift+C     | Copy the result                                       |
| Numpad           | Digits and operators, as typed                        |

`*`, `x` and `×` all multiply; `/` and `÷` divide; `-` and `−` subtract. After an
answer, an operator carries on from it (`+ 5` adds 5 to the answer) and a digit
starts again. Pasted text works: `1,234.5 + 1 000` and `1.234,5` both read.

## Exact arithmetic

A number is a fraction of two integers of any size. `0.1 + 0.2` is `0.3`,
`1/3 * 3` is `1`, and `100!` shows all 158 digits. A result that never ends
(`1/3`, `√2`) shows to 16 significant digits; the main menu's Precision sets 10
to 50. Functions with no exact value (roots, logarithms, trigonometry, powers
with a fractional exponent) are computed to 70 digits and, when the result is
within 1e-60 of a simple fraction, taken for it: `sin(30)` is exactly `0.5`,
`√2 * √2` is exactly `2`. Results over about 200,000 digits, and factorials
above 25,000, are refused with a message.

Percent works as Apple Calculator does:

| Typed       | Result                 |
| ----------- | ---------------------- |
| `50%`       | 0.5                    |
| `200 + 10%` | 220 (10% of 200 added) |
| `200 - 10%` | 180                    |
| `200 * 10%` | 20                     |
| `20% of 50` | 10                     |

Precedence: `+ -`, then `* / mod`, then juxtaposition (`2π`, `2(3)`, `3sin(30)`),
then unary minus, then `^` (right to left), then `! % ° ²`. So `-2^2` is `-4`,
`2^3^2` is `2^9` and `1/2π` is `1/(2π)`. Function arguments are separated by `;`
(`max(1;2)`, `root(27;3)`).

## Scientific

`sin cos tan asin acos atan sinh cosh tanh asinh acosh atanh ln log log2 exp sqrt
cbrt root abs floor ceil round trunc sign fact gcd lcm ncr npr min max mod atan2`,
the constants `π e φ τ`, `ans`, `mem` and `rand`. Angles are in degrees or
radians (Deg and Rad); `30°` is 30 degrees in either. The tangent of 90 degrees
is reported as undefined, not as a huge number. Memory keys: mc, m+, m−, mr.

## Programmer

Whole numbers of 8, 16, 32 or 64 bits, signed or unsigned. HEX, DEC, OCT and BIN
show at once; press a row to type in that base. The bit grid flips bits. Every
result wraps to the word, as hardware does (255 + 1 in an unsigned byte is 0),
and the display says when it did. Operators are C's: `+ - * / % & | ^ ~ << >>`,
and `and or xor not nand nor shl shr rol ror mod`. `0x`, `0b` and `0o` prefixes
work in any base. Division truncates toward zero.

## Convert

Length, weight and mass, temperature, volume, area, speed, time, data (decimal
and binary prefixes), energy, power, pressure, angle, fuel economy and currency.
Conversions are exact (1 in is 25.4 mm). The same conversions work in the
expression field of Basic and Scientific: `5 km to miles`, `72 degF in C`,
`10 usd to gbp`, `(2+3)*10 kg -> lb`.

Currency rates are the European Central Bank's daily reference rates: free, no
key, 30 currencies and the euro. They are downloaded once every 12 hours on a
worker and kept in `$XDG_CACHE_HOME/bingux-calculator/rates.json`. A snapshot is
built into the program, so conversion works on the first launch with no network.
The Convert page says which rates are in use and from what date.

## Files

| Path                                           | What                                                                         |
| ---------------------------------------------- | ---------------------------------------------------------------------------- |
| `$XDG_STATE_HOME/bingux-calculator/state.json` | History (500 entries), mode, angle unit, precision, memory, word size, units |
| `$XDG_CACHE_HOME/bingux-calculator/rates.json` | The last download of the exchange rates                                      |

A state file that cannot be read is moved to `state.json.broken` and the
calculator starts afresh. Files are written whole and renamed, so a crash never
leaves half a file.

## Locale

The decimal mark and the digit grouping come from `LC_ALL`, `LC_NUMERIC` or
`LANG`: `1,234.5` (en), `1.234,5` (de), `1 234,5` with a narrow no-break space
(fr), `12,34,567` (en_IN). A group mark that is a comma or a point keeps a
double-click on the result selecting the whole number; a no-break space does
not, as in any text. The entry field holds exactly what was typed, so grouping
never moves the caret or a selection; the result, the echo and the history show
grouped digits. Copy puts a plain number on the clipboard.

## How it is built

- `bingux_ui::controls::keypad` is the grid of keys, general enough for a PIN
  entry or a dialler. `Keypad::refocus(id)` hands the keyboard back to a field
  after each press.
- `bingux_ui::controls::readout` is the result: as large as fits, scaled down,
  then scrolling, selectable, announced politely to a screen reader.
- `TextField::edits(State<Option<FieldEdit>>)` lets a keypad type into a field
  at the caret without taking its focus (and so without raising a keyboard on a
  touch screen).

## Decisions

- **fend-core** (MIT, units, currency, dates in a text language) was considered
  and not used: its API returns text, it has no hook for Apple's percent
  rules or for a word size, and a second number type would sit beside the exact
  one the keypad modes need. The unit tables here are exact fractions of a base
  unit, which gives the same answers for the categories above.
- **GMP/MPFR (`rug`)** are LGPL and need system libraries; **bigdecimal** and
  **dashu** do not provide the transcendental functions at the precision used
  here. `num-bigint` and `num-rational` (MIT or Apache-2.0, already in the
  lock files of the other apps) hold the numbers, and the functions are series
  in fixed-point integers.
- **ureq** (MIT or Apache-2.0, rustls) downloads the rates; searchd already uses
  it.

## Tests

`cargo test --manifest-path crates/calc/Cargo.toml` runs 141 tests: exactness,
percent, precedence, degrees and radians, undefined results, huge values, word
overflow, locale and paste, unit tables, rates parsing and staleness, and the
saved state (round trip, broken file, older file, history cap).
`bingux-calculator --snapshot DIR` (build with `--features snapshot`) renders
each mode and the history; set `BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-light`
or `prefer-dark`, and point `XDG_STATE_HOME` and `XDG_CACHE_HOME` at an empty
folder.
