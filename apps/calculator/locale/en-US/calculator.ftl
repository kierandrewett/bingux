### Bingux Calculator.
### Source language: American English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `calculator-`.

# The app's name, in the window title, the header and the About window.
calculator-title = Calculator
# One line in the About window.
calculator-description = Calculate, convert units and currencies and work in binary.

## The modes, in the switch at the top of the window

calculator-mode-basic = Basic
calculator-mode-scientific = Scientific
calculator-mode-programmer = Programmer
calculator-mode-convert = Convert

## The display and the entry line

calculator-entry =
    .placeholder = Expression
calculator-entry-programmer =
    .placeholder = Number or expression
calculator-entry-convert =
    .placeholder = Value to convert
calculator-copy-result = Copy result
# The badge shown while a number is in memory (m+).
calculator-memory-badge = M
# Short labels of the angle unit, on a key and in the display.
calculator-angle-radians = Rad
calculator-angle-degrees = Deg
# The word size line of Programmer, such as "HEX  32-bit signed".
# $base (String) - HEX, DEC, OCT or BIN
# $bits (Number) - 8, 16, 32 or 64
# $signed (String) - signed or unsigned
calculator-word-status = { $base }  { $bits }-bit { $signed ->
    [signed] signed
   *[other] unsigned
}
# Shown after the word size line, after a dot, when the result is also a
# printable character.
# $char (String) - the character, such as A
calculator-character-status = Character { $char }
calculator-signed = Signed

## Errors under the entry. The first sentence says what failed.

calculator-divide-by-zero-error = Cannot divide by zero. Change the divisor.
calculator-not-real-error = The result is not a real number. Check the values.
# $function (String) - such as tan
calculator-undefined-error = { $function } has no value here. Change the value.
calculator-incomplete-error = The expression is not finished. Complete it, then try again.
calculator-too-large-error = The result has too many digits. Use smaller numbers.
# $name (String) - what was typed
calculator-unknown-name-error = “{ $name }” is not a known name. Check the spelling.
# $function (String) - such as factorial
calculator-needs-integer-error = { $function } needs a whole number. Change the value.
# $currency (String) - a currency code such as JPY
calculator-no-rate-error = No exchange rate for { $currency } yet. Connect to the internet to update the rates.
calculator-overflow-error = The result does not fit the word size. Choose a larger size.

## History

calculator-history-title = History
calculator-history-empty-title = No history
calculator-history-empty-description = Finished calculations appear here.
calculator-history-clear = Clear
calculator-history-clear-menu = Clear history
calculator-history-use-expression = Use expression
calculator-history-use-result = Use result
calculator-history-copy-expression = Copy expression
calculator-history-delete = Delete
calculator-history-show =
    .tooltip = Show history (Ctrl+H)
calculator-history-hide =
    .tooltip = Hide history (Ctrl+H)

## The main menu

calculator-menu-group-digits = Group digits
calculator-menu-precision = Precision
# $count (Number) - how many significant digits results keep
calculator-menu-digits = { $count } digits
# The commands the window registers: the palette, the shortcuts sheet and the
# main menu show these titles. The keys come from the command itself.
calculator-command-copy = Copy the result
calculator-command-clear = Clear the entry, then everything
calculator-command-history = Show or hide the history

## Convert

calculator-convert-category = Category
# The key that swaps the two units; its face is short.
calculator-convert-swap = Swap
    .a11y = Swap units
calculator-rates-update =
    .tooltip = Update rates

## Keys. The face of a key (sin, x², AND) is a symbol and stays as it is;
## these are what a screen reader and the tooltip say.

calculator-key-delete = Delete
    .a11y = Delete the last character
    .tooltip = Delete (Backspace)
calculator-key-clear-entry =
    .a11y = Clear the entry
    .tooltip = Clear the entry (Escape)
calculator-key-clear-all =
    .a11y = Clear everything
    .tooltip = Clear everything (Escape)
calculator-key-equals =
    .a11y = Equals
    .tooltip = Equals (Enter)
calculator-key-negate = Change sign
calculator-key-point = Decimal point
calculator-key-second = Second functions
calculator-key-angle-radians =
    .a11y = Angles in radians, switch to degrees
calculator-key-angle-degrees =
    .a11y = Angles in degrees, switch to radians
calculator-key-random = Random number
# $digit (String) - A to F
calculator-key-hex-digit = Hex digit { $digit }
calculator-key-percent = Percent
calculator-key-divide = Divide
calculator-key-multiply = Multiply
calculator-key-subtract = Subtract
calculator-key-add = Add
calculator-key-open-bracket = Open bracket
calculator-key-close-bracket = Close bracket
calculator-key-square = Square
calculator-key-cube = Cube
calculator-key-power = Power
calculator-key-exponential = Exponential
calculator-key-reciprocal = Reciprocal
calculator-key-square-root = Square root
calculator-key-cube-root = Cube root
calculator-key-root = Root
calculator-key-natural-logarithm = Natural logarithm
calculator-key-factorial = Factorial
calculator-key-euler-number = Euler’s number
calculator-key-times-ten-to-the-power = Times 10 to the power
calculator-key-pi = Pi
calculator-key-shift-left = Shift left
calculator-key-shift-right = Shift right
calculator-key-rotate-left = Rotate left
calculator-key-rotate-right = Rotate right
calculator-key-and = And
calculator-key-or = Or
calculator-key-exclusive-or = Exclusive or
calculator-key-not = Not
calculator-key-not-and = Not and
calculator-key-not-or = Not or
calculator-key-last-answer = Last answer
calculator-key-remainder = Remainder
calculator-key-clear-memory = Clear memory
calculator-key-add-to-memory = Add to memory
calculator-key-subtract-from-memory = Subtract from memory
calculator-key-recall-memory = Recall memory
calculator-key-ten-to-the-power = 10 to the power
calculator-key-two-to-the-power = 2 to the power
calculator-key-logarithm-base-10 = Logarithm base 10
calculator-key-logarithm-base-2 = Logarithm base 2
calculator-key-sine = Sine
calculator-key-inverse-sine = Inverse sine
calculator-key-cosine = Cosine
calculator-key-inverse-cosine = Inverse cosine
calculator-key-tangent = Tangent
calculator-key-inverse-tangent = Inverse tangent
calculator-key-hyperbolic-sine = Hyperbolic sine
calculator-key-inverse-hyperbolic-sine = Inverse hyperbolic sine
calculator-key-hyperbolic-cosine = Hyperbolic cosine
calculator-key-inverse-hyperbolic-cosine = Inverse hyperbolic cosine
calculator-key-hyperbolic-tangent = Hyperbolic tangent
calculator-key-inverse-hyperbolic-tangent = Inverse hyperbolic tangent
