import io
import os
from pathlib import Path
import re
import subprocess
import sys
from tempfile import TemporaryDirectory
from textwrap import dedent
import unittest

from code_lang import CodeError, Interpreter


ROOT = Path(__file__).resolve().parents[1]
NATIVE_BINARY = os.environ.get("CODE_LANG_TEST_BINARY")


def run(source, *, input_text=""):
    source = dedent(source).lstrip("\n")
    if NATIVE_BINARY:
        with TemporaryDirectory() as directory:
            program = Path(directory) / "test.code"
            program.write_text(source, encoding="utf-8")
            result = cli(str(program), input_text=input_text)
            if result.returncode == 0:
                return result.stdout
            error = re.fullmatch(
                rf"{re.escape(str(program))}: Line ([0-9]+): (.*)\n", result.stderr, re.S
            )
            if result.returncode == 1 and error:
                raise CodeError(int(error[1]), error[2])
            raise AssertionError(f"Unexpected native result {result.returncode}: {result.stderr}")
    output = io.StringIO()
    Interpreter(output, input_stream=io.StringIO(input_text)).run(source)
    return output.getvalue()


def cli(*args, input_text=""):
    command = (
        [str(Path(NATIVE_BINARY).resolve())]
        if NATIVE_BINARY
        else [sys.executable, "-B", "-m", "code_lang"]
    )
    return subprocess.run(
        [*command, *args],
        cwd=ROOT, input=input_text, capture_output=True, text=True, encoding="utf-8",
        check=False, timeout=10,
    )


class InterpreterTests(unittest.TestCase):
    def assert_code_error(self, source, line, message):
        with self.assertRaises(CodeError) as caught:
            run(source)
        self.assertEqual(caught.exception.line, line)
        self.assertIn(message, str(caught.exception))

    def test_variables_arithmetic_comments_and_loop_count_variable(self):
        self.assertEqual(run("""
            note: Whole-line comments are ignored.
            remember count as 2
            remember total as 8
            set total to -2

            do this count times:
                add one to total
            ok
            add count to total
            take 1 from total
            what is total?
        """), "1\n")

    def test_more_and_less_and_both_remember_orders(self):
        self.assertEqual(run("""
            remember 10 as apples
            apples but 5 more
            how many apples?
            remember x as 2.5
            apples but x less
            how many apples?
            remember (apples) as remaining
            what is remaining?
        """), "15\n12.5\n12.5\n")

    def test_but_can_still_be_a_variable(self):
        self.assertEqual(run("remember but as 4\nsay but"), "4\n")

    def test_new_comments_skip_invalid_syntax_and_block_markers(self):
        self.assertEqual(run("""
            do this 2 times:
                comment:
                    ok
                    say "unterminated
                end of comment
                comment on one line: say "ignored"
                say "inside"
            ok
            say "comment:"
        """), "inside\ninside\ncomment:\n")
        self.assert_code_error("comment:\nignored\nend of comment\nsay unknown", 4, "Unknown variable")

    def test_say_on_one_line_writes_without_appending_newlines(self):
        self.assertEqual(run(r'''
            remember on as "C"
            say on one line "A\nB"
            say on one line 2
            say on
            say on one line arrow(right)
        '''), "A\nB2C\n➡️")

    def test_ask_reads_one_line_as_text_and_supports_shorthand_equality(self):
        self.assertEqual(run("""
            remember prompt as "Y/N?"
            ask prompt and remember as answer
            if answer is "Y":
                say "yes"
            otherwise:
                say "no"
            ok
            ask "Next?" and remember as next
            say next
        """, input_text="Y\r\n 42 \n"), "Y/N?\nyes\nNext?\n 42 \n")

    def test_ask_distinguishes_a_blank_answer_from_end_of_input(self):
        source = 'ask "Answer?" and remember as answer\nsay answer'
        self.assertEqual(run(source, input_text="\n"), "Answer?\n\n")
        self.assert_code_error(source, 1, "No input available")

    def test_ask_checks_prompt_and_variable(self):
        self.assert_code_error("ask 1 and remember as answer", 1, "Expected text for the prompt")
        self.assert_code_error(
            'remember answer as 0\nask "Answer?" and remember as answer', 2, "already exists"
        )

    def test_if_branches_and_comparison_types(self):
        self.assertEqual(run("""
            remember items as all of these: 2 named score, true named ready
            if items named ready:
                if items named score is less than 3:
                    say "ready"
                ok
            otherwise:
                say missing
            ok
            if true is equal to 1:
                say missing
            otherwise:
                say "truth is distinct"
            ok
            if 2 is equal to 2.0:
                say "numeric equality"
            ok
            if "turnip" is not equal to "snake":
                say "text inequality"
            ok
        """), "ready\ntruth is distinct\nnumeric equality\ntext inequality\n")

    def test_while_reevaluates_condition_and_skips_false_body(self):
        self.assertEqual(run("""
            while false:
                say missing
            ok
            remember running as true
            while running:
                say "once"
                set running to false
            ok
        """), "once\n")

    def test_two_counter_instructions_and_zero_test(self):
        self.assertEqual(run("""
            remember instruction as 1
            remember left as 3
            remember right as 0
            while instruction is not equal to 0:
                if instruction is equal to 1:
                    if left is equal to 0:
                        set instruction to 0
                    otherwise:
                        left but 1 less
                        set instruction to 2
                    ok
                otherwise:
                    right but 1 more
                    set instruction to 1
                ok
            ok
            say left
            say right
        """), "0\n3\n")

    def test_truth_literals_and_text(self):
        self.assertEqual(run(r'''
            remember true as flag
            say flag
            say false
            say "a"
            say "comma, named, \"quote\" and café"
        '''), 'true\nfalse\na\ncomma, named, "quote" and café\n')

    def test_forbidden_character_in_source(self):
        for source, line in (
            ("§", 1),
            ('say "§"', 1),
            ('say "§§§§§§§ + §"', 1),
            ("note: §", 1),
            ("note: §§§§§§§", 1),
            ("comment on one line: §", 1),
            ('comment:\nsay "§§§§§§§"\nend of comment', 2),
            ('say "§§§§§§', 1),
            ("do this 0 times:\n    say §\nok", 2),
            ("@\nnote: §\n§", 2),
            (r'say "\u00a7"', 1),
        ):
            with self.subTest(source=source):
                self.assert_code_error(source, line, "We must never speak of §!")

    def test_exactly_seven_section_signs_become_a_snake_in_text(self):
        self.assertEqual(run(r'''
            say "§§§§§§§"
            remember snakes as "§§§§§§§ and §§§§§§§"
            say snakes
            say "§§§§§§\u00a7"
        '''), "🐍\n🐍 and 🐍\n🐍\n")

    def test_section_sign_runs_must_have_exactly_seven_characters(self):
        for count in (6, 8, 14):
            with self.subTest(count=count):
                self.assert_code_error(f'say "{"§" * count}"', 1, "We must never speak of §!")

    def test_arrow_calls_return_text_values(self):
        self.assertEqual(run("""
            remember arrow(right) as direction
            remember map as all of these: arrow(left) named west, arrow(up)
            if direction is equal to "➡️":
                say map named west
            ok
            what is the second of map?
            remember arrow as "a variable"
            say arrow
        """), "⬅️\n⬆️\na variable\n")

    def test_arrow_calls_require_one_of_four_direction_words(self):
        for direction in ('north', '"up"', 'UP', '1', ''):
            with self.subTest(direction=direction):
                self.assert_code_error(
                    f"say arrow({direction})", 1, "Expected arrow direction up, down, left, or right"
                )
        self.assert_code_error("say arrow(up, down)", 1, "Expected ')'")

    def test_names_and_positions_share_the_same_item(self):
        self.assertEqual(run("""
            remember items as all of these: "a" named letter, 3 named count, "c"
            put 5 into items named count
            what is the second of items?
            what is items named count?
            remove position 1 of items
            put false into items
            what is the first of items?
            what is the third of items?
            how many are in items?
        """), "5\n5\n5\nfalse\n3\n")

    def test_positions_above_ten(self):
        self.assertEqual(run("""
            remember items as all of these: 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11
            what is the tenth of items?
            what is position 11 of items?
        """), "10\n11\n")

    def test_nested_lookup_and_reuse_of_names(self):
        self.assertEqual(run("""
            remember items as all of these:
                all of these:
                    "inner" named label
                ok named label
                (all of these: "second" named label)
            ok
            what is items named label named label?
            what is (the second of items) named label?
            put "updated" into (the second of items) named label
            remove the first of items named label
            say items
        """), 'inner\nsecond\n[[] named label, ["updated" named label]]\n')

    def test_empty_collection(self):
        self.assertEqual(run("""
            remember items as all of these:
            ok
            how many are in items?
            for each item in items:
                say item
            ok
            say items
        """), "0\n[]\n")

    def test_collection_items_can_read_variables_and_grouped_lookups(self):
        self.assertEqual(run("""
            remember x as 4
            remember old as all of these: 5 named x
            remember new as all of these: x named x, (old named x)
            say new
        """), "[4 named x, 5]\n")

    def test_nested_blocks_and_temporary_iteration_variables(self):
        self.assertEqual(run("""
            remember item as "outside"
            remember items as all of these: 1, 2 named second
            remember total as 0
            for each item in items:
                do this item times:
                    add item to total
                ok
                for each item in items:
                    add item to total
                ok
                say item
            ok
            say item
            say total
        """), "1\n2\noutside\n11\n")
        self.assert_code_error("""
            remember items as all of these: 1
            for each item in items:
            ok
            say item
        """, 4, "Unknown variable 'item'")

    def test_named_update_during_iteration_and_other_collection_can_resize(self):
        self.assertEqual(run("""
            remember items as all of these: 1, 2 named next
            remember other as all of these:
            ok
            remember alias as items
            for each item in items:
                put 9 into alias named next
                put item into other
            ok
            put 3 into alias
            say items
            say other
        """), "[1, 9 named next, 3]\n[1, 9]\n")

    def test_resize_during_iteration_is_rejected_even_through_an_alias(self):
        for mutation in ("put 2 into alias", "remove the first of alias"):
            with self.subTest(mutation=mutation):
                self.assert_code_error(f"""
                    remember items as all of these: 1
                    remember alias as items
                    for each item in items:
                        {mutation}
                    ok
                """, 4, "Cannot change a collection's size")

    def test_inner_loop_does_not_release_outer_traversal_guard(self):
        self.assert_code_error("""
            remember items as all of these: 1
            for each outer in items:
                for each inner in items:
                ok
                put 2 into items
            ok
        """, 5, "Cannot change a collection's size")

    def test_cycle_display_and_shared_references(self):
        self.assertEqual(run("""
            remember child as all of these: 1
            remember parent as all of these: child, child
            say parent
            put parent into child
            say child
        """), "[[1], [1]]\n[1, [<cycle>, <cycle>]]\n")

    def test_duplicate_names_report_the_second_entry_line(self):
        self.assert_code_error("""
            remember items as all of these:
                1 named repeated,

                note: This counts toward the source line number.
                2 named repeated
            ok
        """, 5, "Duplicate item name 'repeated'")

    def test_removed_name_is_gone(self):
        for statement in ("what is items named key?", "put 3 into items named key"):
            with self.subTest(statement=statement):
                self.assert_code_error(f"""
                    remember items as all of these: 1 named key
                    remove the first of items
                    {statement}
                """, 3, "No item named 'key'")

    def test_invalid_positions(self):
        for index in ("-1", "0", "2"):
            with self.subTest(index=index):
                self.assert_code_error(f"""
                    remember items as all of these: 1
                    what is position {index} of items?
                """, 2, f"Position {index} is out of range")

    def test_malformed_programs(self):
        for source, line, message in (
            ('say "unterminated', 1, "Unterminated text"),
            (r'say "bad \z"', 1, "Invalid text literal"),
            ("remember items as all of these:\n  1", 1, "Missing ok for collection"),
            ("do this 0 times:\n  say 1", 1, "Missing ok for loop"),
            ("do this 0 times:\n  nonsense\nok", 2, "Unknown instruction"),
            ("while false:\n  nonsense\nok", 2, "Unknown instruction"),
            ("if true:\n  say 1", 1, "Missing ok for if"),
            ("if true:\notherwise\nok", 2, "Expected ':'"),
            ("if true:\notherwise:\notherwise:\nok", 3, "Unexpected otherwise"),
            ("if true:\nok otherwise:", 2, "Unexpected 'otherwise'"),
            ("while false:\notherwise:\nok", 2, "Unexpected otherwise"),
            ("if 1 is around 2:\nok", 1, "Expected a comparison"),
            ("note: comment\n\nok", 3, "Unexpected ok"),
            ("say 1\ncomment:\nignored", 2, "Missing end of comment"),
            ("note: comment\nend of comment", 2, "Unexpected end of comment"),
            ("what is apples", 1, "Expected '?'"),
            ("remember x as 2\nx but 1", 2, "Expected 'more' or 'less'"),
            ("remember items as all of these: 1 2", 1, "Unexpected '2'"),
        ):
            with self.subTest(source=source):
                self.assert_code_error(source, line, message)

    def test_type_and_variable_errors(self):
        for source, line, message in (
            ("set missing to 3", 1, "Unknown variable 'missing'"),
            ("remember x as 1\nremember x as 2", 2, "already exists"),
            ("remember x as true\nx but 1 more", 2, "Expected a number"),
            ('remember x as "text"\nhow many x?', 2, "Expected a number"),
            ("how many are in 4?", 1, "Expected a collection"),
            ("do this true times:\nok", 1, "non-negative whole number"),
            ("do this -1 times:\nok", 1, "non-negative whole number"),
            ("if 1:\nok", 1, "Expected true or false as the condition"),
            ('while "two" is less than 3:\nok', 1, "Expected a number"),
            ("if (all of these: 1) is equal to 1:\nok", 1, "Equality requires"),
        ):
            with self.subTest(source=source):
                self.assert_code_error(source, line, message)

    def test_interpreter_can_run_multiple_programs_independently(self):
        output = io.StringIO()
        interpreter = Interpreter(output)
        interpreter.run("remember x as 1\nsay x")
        interpreter.run("remember x as 2\nsay x")
        self.assertEqual(output.getvalue(), "1\n2\n")


class CLITests(unittest.TestCase):
    @unittest.skipUnless(os.name == "posix", "The launcher uses a Unix shebang")
    def test_launcher_runs_a_script(self):
        result = subprocess.run(
            ["./code-lang", "examples/apples.code"],
            cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=False, timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertEqual(result.stdout, "15\n")

    def test_all_examples_with_exact_output(self):
        expected = {
            "apples": "15\n",
            "arrows": "⬆️\n⬇️\n⬅️\n➡️\n",
            "ask": "Continue? (Y/N)\nContinuing.\n",
            "gob": "Gob's Program: Y/N?\n",
            "one_line": "The turnip goes ➡️",
            "snake": "🐍\n",
            "turnips": "3\n2\nLast turnip!\nTurnip complete.\n",
            "hello": "Hello\nWorld\n",
            "counter": "5\n",
            "comments": "Hello from Code!\n",
            "countdown": "5\n4\n3\n2\n1\nBlast off!\n",
            "nested_loops": "12\n",
            "inventory": "apple\nbanana\n12\n4\napple\n20\n5\norange\n",
            "shopping": "Shopping list:\nmilk\nbread\neggs\n3\nTotal items:\n4\nUpdated quantity:\n6\n",
            "nested_collections": 'oats\n["rice", "porridge" named breakfast]\n["pear", "banana"]\ntrue\n',
        }
        example_input = {"ask": "Y\n", "gob": "N\n"}
        self.assertEqual({path.stem for path in (ROOT / "examples").glob("*.code")}, expected.keys())
        for program, output in expected.items():
            with self.subTest(program=program):
                result = cli(f"examples/{program}.code", input_text=example_input.get(program, ""))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stderr, "")
                self.assertEqual(result.stdout, output)

    def test_error_has_filename_and_physical_line_number(self):
        with TemporaryDirectory() as directory:
            program = Path(directory) / "bad.code"
            program.write_text("note: a comment\n\nsay unknown\n", encoding="utf-8")
            result = cli(str(program))
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, "")
            self.assertEqual(result.stderr, f"{program}: Line 3: Unknown variable 'unknown'\n")

    def test_forbidden_character_stops_before_any_output(self):
        with TemporaryDirectory() as directory:
            program = Path(directory) / "forbidden.code"
            program.write_text('say "hello"\n\nnote: §\n', encoding="utf-8")
            result = cli(str(program))
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, "")
            self.assertEqual(result.stderr, f"{program}: Line 3: We must never speak of §!\n")

    def test_missing_file_and_version(self):
        with TemporaryDirectory() as directory:
            missing = Path(directory) / "missing.code"
            result = cli(str(missing))
            self.assertEqual(result.returncode, 1)
            self.assertIn(str(missing), result.stderr)
        result = cli("--version")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "Code 0.2.0\n")

    def test_readme_examples_match_documented_output(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        examples = re.findall(r"```code\n(.*?)```\s*```text\n(.*?)```", readme, re.S)
        self.assertEqual(len(examples), len(re.findall(r"^```code$", readme, re.M)))
        self.assertTrue(examples)
        for source, output in examples:
            with self.subTest(source=source):
                self.assertEqual(run(source), output)


if __name__ == "__main__":
    unittest.main()
