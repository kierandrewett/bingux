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

Every word the window shows is a Fluent message in
`apps/calculator/locale/en-US/calculator.ftl` (and bingux-ui's catalogue for the
shared parts), asked for with `tr!`; a translation goes in
`apps/calculator/locale/<language>/calculator.ftl`. Key faces (`sin`, `x²`,
`AND`), the base names `HEX`, `DEC`, `OCT`, `BIN`, and the detail of a syntax,
domain or unit error (from `bingux_calc`, in English) are not translated.
Calculator is the reference migration in `docs/bingux-ui.md`, "Localisation".

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

## Failure modes

The list below was written for the engine and the window, and each mode names
the test that holds it. A new feature adds its modes here first, then its test
(the repository's testing policy). `E` rows are the engine (`crates/calc`), `W`
rows the window (`apps/calculator`) and the toolkit parts it uses.

| Id  | What can go wrong                                                                                              | What is expected                                                                                        | Held by                                                                                                                                                                                                                                                |
| --- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| E1  | A decimal comes out inexact (`0.1 + 0.2`)                                                                      | Exact fractions, shown to the end                                                                       | `decimals_add_exactly`, `point_one_plus_point_two_is_point_three`, `exact_decimals_are_shown_in_full`                                                                                                                                                  |
| E2  | A result that never ends (`1/3`) is cut wrongly or never                                                       | Cut at the precision, rounded half away from zero, flagged as rounded                                   | `division_is_exact_then_cut_at_the_precision`, `one_third_is_cut_at_the_precision`, `rounding_is_reported`, `precision_is_adjustable`                                                                                                                  |
| E3  | Division by zero, `mod 0`, `0^-1`                                                                              | A named error, never a number or a panic                                                                | `errors_say_what_failed`, `division_by_zero_fails`, `zero_to_a_negative_power_divides_by_zero`                                                                                                                                                         |
| E4  | A value outside a function's domain (`√-1`, `ln 0`, `asin 2`, `tan 90°`)                                       | A named error that says what to change                                                                  | `errors_say_what_failed`, `the_tangent_is_undefined_at_a_right_angle`, `log_of_zero_and_negatives`, `arcsine_domain`, `the_root_of_a_negative_is_not_real`                                                                                             |
| E5  | A result with no end in size (`10^1000000`, `1e999999999`, `1000000!`)                                         | Refused with a message, in bounded time and memory                                                      | `errors_say_what_failed`, `huge_powers_are_refused`, `factorials`                                                                                                                                                                                      |
| E6  | A series answer leaves noise (`sin 30` as `0.49999...`, `√2 * √2` as `2.0000...1`) or zeroes a tiny true value | Exact when within 1e-60 of a simple fraction, and a tiny value is kept                                  | `degrees_give_exact_answers_at_the_common_angles`, `sqrt_two_squared_is_two`, `a_tiny_sine_is_not_zeroed`, `a_tiny_result_is_not_zeroed_without_a_scale`, `a_value_near_a_simple_fraction_is_that_fraction`                                            |
| E7  | Percent means different things beside `+`, `*` and `of`                                                        | Apple Calculator's rules                                                                                | `percent_follows_apple_calculator`                                                                                                                                                                                                                     |
| E8  | Precedence, unary minus, right-associative `^`, juxtaposition                                                  | As in the Exact arithmetic section                                                                      | `precedence_and_brackets`, `implicit_multiplication`, parser `precedence`, `implicit_multiplication_binds_tighter_than_division`, `postfix_operators`                                                                                                  |
| E9  | Unfinished or wrong input typed live                                                                           | An unfinished one previews what is finished; a real mistake is reported where it is; nesting is limited | `the_preview_shows_what_is_finished`, `the_preview_reports_real_mistakes`, `an_unfinished_expression_is_incomplete`, `mistakes_are_reported`, `nesting_is_limited`, `two_numbers_in_a_row_need_an_operator`                                            |
| E10 | A paste uses another locale's marks, a no-break space, or a comma that is not a group                          | Read as the number meant, or the comma ends the number                                                  | `grouped_numbers_from_a_paste`, `european_numbers`, `a_comma_that_is_not_a_group_ends_the_number`, `a_pasted_grouped_expression_reads`, `german_input`                                                                                                 |
| E11 | Look-alike characters (`×`, `x`, `−`, `π`), a name that starts with `x`, a stray character                     | Same meaning as the ASCII form; a stray one is named with its place                                     | `every_multiplication_sign`, `an_x_that_starts_a_name_is_a_name`, `unicode_minus_and_pi`, `a_stray_character_is_reported_with_its_place`, `a_trailing_equals_sign_is_ignored`                                                                          |
| E12 | A word of 8 to 64 bits overflows, shifts by a negative or too large count, or divides negatives                | Wraps to the word and says so; negative shifts refused; division truncates                              | `unsigned_arithmetic_wraps`, `signed_arithmetic_wraps`, `division_truncates_toward_zero`, `shifts_and_rotations`, `a_literal_too_big_for_the_word_wraps_and_says_so`, `sixty_four_bit_limits`                                                          |
| E13 | A digit outside the base, or a prefix that disagrees with the base                                             | Reported; `0x`, `0b`, `0o` work in any base                                                             | `a_digit_outside_the_base_is_reported`, `numbers_in_each_base`, `the_entry_base_decides_how_digits_are_read`                                                                                                                                           |
| E14 | A smaller word cuts a value the person still wants                                                             | The value is kept when it fits and cut when it does not                                                 | `changing_the_word_keeps_or_cuts_the_value`, `changing_the_word_keeps_the_value_it_can`                                                                                                                                                                |
| E15 | A unit conversion is inexact, offset (temperature) or reciprocal (fuel economy), or a name means two units     | Exact fractions; zero points; the category hint decides                                                 | `lengths_are_exact`, `temperatures_use_their_zero_points`, `fuel_economy_is_reciprocal`, `a_unit_can_belong_to_two_categories_and_the_hint_decides`, `units_are_found_by_what_people_type`, `every_unit_id_is_unique_in_its_category_and_has_a_factor` |
| E16 | A typed conversion names no unit, or two plain numbers                                                         | Not a conversion; the expression is judged as arithmetic                                                | `typed_conversions` (`5 km to blorp`, `5 to 6`)                                                                                                                                                                                                        |
| E17 | Currency with no rates, a bad feed, rates too old, no network                                                  | Built-in snapshot is used; a bad feed is refused; old rates are marked stale                            | `currency_needs_rates`, `the_ecb_feed_is_read`, `a_feed_without_rates_is_refused`, `rubbish_rates_are_skipped`, `the_bundled_snapshot_covers_every_listed_currency`, `staleness`, `the_euro_is_one`, `rates_notes_say_where_the_rates_are_from`        |
| E18 | The state file is missing, broken, from an older version, or half written                                      | Defaults; moved to `.broken`; missing fields default; written whole and renamed                         | `a_missing_file_gives_the_defaults`, `a_broken_file_is_moved_aside`, `an_older_file_with_fewer_fields_still_loads`, `a_save_leaves_no_temporary_file_and_replaces_the_old_one_whole`                                                                   |
| E19 | History grows without end, repeats, or a failed Enter records something                                        | 500 entries, no immediate repeat, a failed calculation changes nothing                                  | `the_same_calculation_twice_is_kept_once_and_the_list_is_capped`, `a_failed_equals_changes_nothing`, `history_survives_a_restart`, `history_is_per_family_and_can_be_edited`, `programmer_history_keeps_its_base_and_word`                             |
| E20 | Locale marks are wrong for the person's language                                                               | `LC_ALL`, `LC_NUMERIC`, `LANG`, in that order; unknown and POSIX mean English                           | locale `*` tests, `locale_marks`, `the_locale_shapes_the_answer`                                                                                                                                                                                       |
| E21 | A saved window size is missing                                                                                 | The default size                                                                                        | `the_window_size_is_kept_and_a_file_without_it_means_the_default_size`                                                                                                                                                                                 |
| W1  | A key typed on the pad lands somewhere other than the caret, or steals the keyboard from the field             | Text goes in at the caret, replacing a selection, and the field keeps the keyboard                      | `Calc::type_text` through `TextField::edits`; checked in the window (see Proof below)                                                                                                                                                                  |
| W2  | An operator after an answer starts again, or brackets wrap wrongly                                             | An operator continues the answer; a digit starts afresh                                                 | `an_operator_continues_an_answer`, `brackets_that_wrap_the_whole_text`                                                                                                                                                                                 |
| W3  | A result wider than the display is cut                                                                         | Scaled down, then scrolls; selectable; announced politely                                               | `Readout` `fit` tests: `text_that_fits_is_full_size`, `text_that_does_not_fit_shrinks_to_fit`, `text_that_would_get_too_small_scrolls_at_the_smallest_size`, `an_unmeasured_box_changes_nothing`                                                       |
| W4  | The history panel is pushed out of a wide window by a full-width calculator                                    | The calculator takes the width the panel leaves                                                         | screenshot at 880x660 (Proof below)                                                                                                                                                                                                                    |
| W5  | Programmer needs more height than the common window has                                                        | All of it shows at 880x660; it scrolls only below that                                                  | screenshot at 880x660 (Proof below)                                                                                                                                                                                                                    |
| W6  | A key with a tooltip stops sharing its row (the tip's box shrinks the key to its label)                        | Keys with and without a tooltip are the same width                                                      | `keypad` test `a_key_with_a_tooltip_keeps_its_share_of_the_row`                                                                                                                                                                                        |
| W7  | `--mode` is missing its value, names no mode, or arrives while the window is open                              | Missing value: usage and exit 2. No mode: opens as usual. Open window: raised and switched              | `an_address_option_names_an_address`, `an_address_option_without_a_value_is_refused`, `a_launch_names_a_mode_by_its_address`, `an_unknown_or_missing_mode_changes_nothing`, `every_mode_has_a_route_and_the_route_names_it`                            |
| W8  | A narrow window (390 px) has no room for the pads or the history                                               | Pads stack; history is a screen of its own                                                              | screenshots at 390x800 (Proof below)                                                                                                                                                                                                                   |

## Tests

`cargo test --manifest-path crates/calc/Cargo.toml` runs 141 tests: exactness,
percent, precedence, degrees and radians, undefined results, huge values, word
overflow, locale and paste, unit tables, rates parsing and staleness, and the
saved state (round trip, broken file, older file, history cap).
`bingux-calculator --snapshot DIR` (build with `--features snapshot`) renders
each mode and the history; set `BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-light`
or `prefer-dark`, and point `XDG_STATE_HOME` and `XDG_CACHE_HOME` at an empty
folder.
