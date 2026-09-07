"""Checks the evidence parser's spacing/style contract, not Ferrite app behavior."""
import json
from pathlib import Path
import runpy
import unittest

RENDER = runpy.run_path(str(Path(__file__).with_name("render-cli-capture.py")))


def parse(text, columns=24, rows=2):
    styles = [RENDER["fresh"]()]
    ids = {json.dumps(styles[0], sort_keys=True): 0}
    unknown = set()
    f = RENDER["parse"]({"time": 0, "meta": f"{columns},{rows},0,0,1,0,0,0", "ansi": text}, styles, ids, unknown)
    return f, styles, unknown


class GridContract(unittest.TestCase):
    def test_literal_spaces_are_not_collapsed(self):
        f, _, unknown = parse("  one  two   three    \n")
        self.assertEqual("".join(c[0] for c in f["cells"][0]), "  one  two   three      ")
        self.assertFalse(unknown)

    def test_wide_character_and_combining_mark_keep_column_positions(self):
        f, _, _ = parse("a界e\u0301 z\n")
        self.assertEqual(f["cells"][0][:7], [["a",0,1],["界",0,2],["",0,0],["e\u0301",0,1],[" ",0,1],["z",0,1],[" ",0,1]])

    def test_tab_expands_to_next_terminal_stop(self):
        f, _, _ = parse("  \tx\n")
        self.assertEqual(f["cells"][0][8][0], "x")
        self.assertTrue(all(c[0] == " " for c in f["cells"][0][:8]))

    def test_nonbreaking_space_remains_distinct(self):
        f, _, _ = parse("a\u00a0b\n")
        self.assertEqual(f["cells"][0][1], ["\u00a0",0,1])

    def test_colour_styles_do_not_introduce_spaces_at_boundaries(self):
        f, styles, unknown = parse("\x1b[1;38;2;12;34;56mBold\x1b[0m, x\n")
        self.assertEqual("".join(c[0] for c in f["cells"][0]).rstrip(), "Bold, x")
        bold = styles[f["cells"][0][0][1]]
        self.assertTrue(bold["bold"])
        self.assertEqual(bold["fg"], "#0c2238")
        self.assertFalse(styles[f["cells"][0][4][1]]["bold"])
        self.assertFalse(unknown)

    def test_styles_carry_across_rows_and_hyperlinks_are_metadata(self):
        f, styles, _ = parse("\x1b[2mA\n\x1b]8;;https://example.com\x1b\\B\x1b]8;;\x1b\\C\n")
        self.assertTrue(styles[f["cells"][1][0][1]]["dim"])
        self.assertEqual(styles[f["cells"][1][0][1]]["link"], "https://example.com")
        self.assertIsNone(styles[f["cells"][1][1][1]]["link"])

    def test_background_on_explicit_blank_cells_is_preserved(self):
        f, styles, _ = parse("\x1b[48;5;237m   \x1b[0m\n")
        self.assertEqual(styles[f["cells"][0][2][1]]["bg"], "index:237")
        self.assertEqual(styles[f["cells"][0][3][1]]["bg"], "default")

    def test_unhandled_attributes_and_resize_overflow_are_reported(self):
        _, _, unknown = parse("\x1b[4:3mabcde\n", columns=4)
        self.assertEqual(unknown, {"SGR:4:3", "grid-overflow:5>4"})


if __name__ == "__main__":
    unittest.main()
