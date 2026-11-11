# Code- often stylised as "Code"

Code v0.2 is an English-like programming language with variables, arithmetic, explicit blocks, and ordered collections. It ships with a standalone Rust interpreter for Apple silicon Macs, a browser playground, and a Python reference interpreter. [Read the HTML guide](https://mcfr4g.github.io/code-lang/).

## Run a program

Open a terminal in this project folder. On an Apple silicon Mac, run the included native binary.

```sh
./bin/code-lang examples/apples.code
./bin/code-lang examples/snake.code
./bin/code-lang examples/arrows.code
./bin/code-lang examples/ask.code
./bin/code-lang examples/turnips.code
```

The binary runs `.code` scripts directly without Python or Cargo installed. The Python version uses Python 3.10 or newer and the standard library. Run it with `./code-lang examples/apples.code` on macOS or Linux, or with `python3 -m code_lang examples/apples.code`.

The CLI reads UTF-8 source files and prints results to standard output. Program errors go to standard error with the filename and physical line number, including any preceding blank or comment lines. A failed run exits with status 1.

## Build the Rust interpreter

With Rust and Cargo installed, build from the project folder. Cargo uses the dependency versions recorded in `native/Cargo.lock`.

```sh
cargo build --release --locked --manifest-path native/Cargo.toml
./native/target/release/code-lang examples/apples.code
```

On Windows, run `.\native\target\release\code-lang.exe examples\apples.code` after the build. The included binary was built and tested on Apple silicon macOS. For other platforms, build the source for that platform.

Run the shared test suite against each interpreter and the Rust runtime tests from the project folder.

```sh
python3 -m unittest discover -s tests -v
CODE_LANG_TEST_BINARY=./bin/code-lang python3 -m unittest discover -s tests -v
cargo test --locked --manifest-path native/Cargo.toml
```

Set `CODE_LANG_TEST_BINARY` to your own binary path if you built from source. Python is required for the shared test harness.

## Run Code in the browser

Open the [browser playground](https://mcfr4g.github.io/code-lang/#playground), choose a bundled example, and click **Run**. You can edit the code before running it. **Tab** indents with four spaces. Press **Esc**, then **Tab** to move focus out of the editor. **Stop** interrupts a run, and the playground stops execution after three seconds. It allows up to 64 KiB each for source, answers, and output.

To rebuild the playground, use a Rust toolchain with the `wasm32-unknown-unknown` target installed and use Node.js to package the result. The commands below select the same Rustup toolchain for Cargo and the Rust compiler.

```sh
rustup target add wasm32-unknown-unknown
RUSTC="$(rustup which rustc)" "$(rustup which cargo)" rustc --locked --profile browser \
  --target wasm32-unknown-unknown --lib --manifest-path native/Cargo.toml \
  -- -C link-arg=--max-memory=67108864
node native/bundle-web.mjs native/target/wasm32-unknown-unknown/browser/code_lang.wasm
node native/test-web.mjs ./bin/code-lang
```

If you built a native binary from source, pass that executable to `test-web.mjs` instead. The test runs each bundled example in a worker and compares its output with the native binary.

## The snake rule

Inside a string, a run of exactly seven consecutive `§` characters becomes one `🐍`. String escapes count after decoding. A shorter or longer run, or any `§` outside a string, stops the program before any instructions run with the message “We must never speak of §!”

```code
say "§§§§§§§"
```

```text
🐍
```

## Variables and arithmetic

```code
remember 10 as apples
apples but 5 more
how many apples?

remember x as 3
apples but x less
what is apples?
```

```text
15
12
```

| Syntax | Behavior |
| --- | --- |
| `remember score as 0` or `remember 0 as score` | Declare a variable. |
| `set score to 10` | Replace the value of a declared variable. |
| `add 5 to score` or `score but 5 more` | Add a number to the stored value. |
| `take 2 from score` or `score but 2 less` | Subtract a number from the stored value. |
| `say score` or `what is score?` | Print a value. |
| `say on one line score` | Print a value without adding a line break. |
| `how many score?` | Print the value of a numeric variable. |
| `do this 5 times:` | Repeat a block that ends with `ok`. |
| `note: a whole-line comment` | Skip the line. |

Use `remember (source) as target` to declare from a variable in value-first order. A bare identifier immediately before `as` uses the original variable-first form. Variables must be declared before use and may be declared once per run. Identifiers use ASCII letters, digits, and underscores and cannot start with a digit. Commands and names are case-sensitive. Command words retain their usual meaning at the start of a statement.

Use `say on one line` to join several values on the same output line. It keeps line breaks already in the value. Try [one_line.code](examples/one_line.code).

Numbers can be integers or decimals, including negative values such as `-2.5`. Repeat counts must be non-negative integers and can come from a variable. The original `add one to score` spelling is supported. Truth literals are `true` and `false`. All text, including a single letter, uses double quotes and JSON-style escapes such as `\"` and `\n`. Indentation and blank lines are optional.

## Comments

Use `comment on one line:` for a single comment line. Put `comment:` and `end of comment` on their own lines to enclose a block of text. The first standalone `end of comment` closes the block. The original `note:` spelling still works for single-line comments.

```code
comment:
    This is a multiline comment.
    say "This line is ignored."
end of comment

comment on one line: Only the next line produces output.
say "Hello from Code!"
```

```text
Hello from Code!
```

## Interactive input

`ask "Your name?" and remember as name` prints the prompt on its own line, reads one response, and declares `name` as text. Enter your answer in Terminal or use the **Answer** field that appears in the browser playground. Blank answers are stored as empty text. Try [ask.code](examples/ask.code).

## Conditions and while loops

`if` runs its block when its condition is true. An optional `otherwise:` runs when the condition is false. One `ok` closes both branches. `while` checks its condition before every iteration and exits when it becomes false. It can run indefinitely if the condition stays true.

`if answer is "Y":` is short for `if answer is equal to "Y":`. Text comparisons are case-sensitive.

```code
remember 3 as turnips

while turnips is greater than 0:
    if turnips is equal to 1:
        say "Last turnip!"
    otherwise:
        how many turnips?
    ok
    turnips but 1 less
ok

say "Turnip complete."
```

```text
3
2
Last turnip!
Turnip complete.
```

Use a truth value directly, as in `if ready:`, or compare two values. The comparison phrases are `is equal to`, `is not equal to`, `is less than`, and `is greater than`. Ordering compares numbers. Equality compares numbers, text, or truth values. Integers and decimals can compare equal, and truth values are distinct from numbers.

With arbitrarily large integer counters, these statements are enough to simulate a two-counter machine. A `while` loop runs until the instruction number reaches a halt state. Nested `if` branches dispatch instructions, test a counter for zero, and update the counters and the instruction number. The Python interpreter is bounded by the memory available to it.

## Collections

Every item has a 1-based position and may have a name. Names are bare identifiers and are unique within their own collection. A name identifies the same item as its position. Appending adds an unnamed item at the end. A named update requires an existing name and preserves the item's position. Removing an item drops its name and shifts later positions down.

```code
remember inventory as all of these:
    "apple" named fruit,
    12 named quantity,
    "banana",
    5
ok

what is the first of inventory?
what is inventory named quantity?
how many are in inventory?

put "orange" into inventory
put 20 into inventory named quantity
remove the third of inventory

for each item in inventory:
    say item
ok
```

```text
apple
12
4
apple
20
5
orange
```

Use `remember scores as all of these: 10, 20, 30` for a single-line collection. Separate its items with commas. In a multiline collection, place one item per line, optionally followed by a comma, and close with `ok`. A multiline collection with no items is empty. Positional access supports `the first of` through `the tenth of` and `position N of` for any literal integer position. Missing names and out-of-range positions raise errors.

### Nested collections

Wrap inline nested literals and item values read by name in parentheses so the outer item's `named` label is unambiguous. A multiline nested literal closes with its own `ok`, which can be followed by the item's name. Chaining `named` looks up each successive name. In `the first of pantry named fruit`, the position applies to the value named `fruit`. Parentheses let you name an item inside a positional result, as in `(the first of pantry) named kind`.

```code
remember pantry as all of these:
    all of these:
        "rice",
        "oats" named breakfast
    ok named grains,
    (all of these: "apple", "pear") named fruit
ok

what is the second of pantry named grains?
put "banana" into (pantry named fruit)
put "porridge" into pantry named grains named breakfast
remove position 1 of pantry named fruit
say pantry
```

```text
oats
[["rice", "porridge" named breakfast] named grains, ["pear", "banana"] named fruit]
```

In a `put` target, a final ungrouped `named` lookup updates that item. Group the lookup as in `put "banana" into (pantry named fruit)` to append to the nested collection it returns. Collections are shared by reference when assigned or inserted. Printing a collection shows its entries in order and displays circular references as `<cycle>`.

### For-each loops

A `for each` loop visits values in collection order, including named items. Its loop variable is temporary. If that variable already exists, its previous value is restored on exit. A collection cannot be appended to or shortened while it is being traversed, including through another variable that refers to it. Named updates are allowed and are visible when a later iteration reaches the updated item.

## Advanced features

### Arrow functions

`arrow(up)`, `arrow(down)`, `arrow(left)`, and `arrow(right)` each return one emoji as text. You can print the result, store it in a variable, or put it in a collection.

```code
say arrow(up)
say arrow(down)
say arrow(left)
say arrow(right)
```

```text
⬆️
⬇️
⬅️
➡️
```

## Limits in v0.2

- Arithmetic is limited to the update commands and their `but … more` and `but … less` equivalents. Decimal arithmetic uses Python floating-point precision.
- Positions use integer literals or the supported English ordinals. Collection mutation supports unnamed append, named replacement, and positional removal.
- Comments occupy their own lines. Text literals occupy a single source line. The parser checks all blocks before execution, including branches and loops that never run.
- Comparisons are available in conditions. General expression operators and user-defined functions are outside v0.2.
- The native interpreter limits parser nesting to 128 levels and applies the same limit when printing nested collections.

The [examples folder](examples/) contains runnable programs. The test suite checks the CLI output for each one and verifies the programs shown above.

## License

Code is available under the [WTFPL v2](LICENSE). It comes without any warranty, to the extent permitted by applicable law. See [third-party notices](docs/THIRD-PARTY-NOTICES.txt) for bundled dependencies.
