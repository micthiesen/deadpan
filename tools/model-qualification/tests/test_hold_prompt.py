import builtins
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


QUALIFICATION = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(QUALIFICATION))
SPEC = importlib.util.spec_from_file_location("deadpan_hold_prompt", QUALIFICATION / "mlx_backend.py")
backend = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(backend)


def constraints(motion="still", instructions=None):
    value = {"video": {"frames": 25, "frame_rate": {"numerator": 24, "denominator": 1},
                       "width": 512, "height": 320},
             "conditioning": "bridge", "motion": motion}
    if instructions is not None:
        value["instructions"] = instructions
    return value


class Tokenizer:
    def __init__(self, count, pad_token_id=7):
        self.count = count
        self.pad_token_id = pad_token_id
        self.calls = []

    def encode(self, text, **kwargs):
        self.calls.append((text, kwargs))
        return list(range(self.count))


class HoldPromptTests(unittest.TestCase):
    def test_motion_changes_prompt_while_preserving_composition_and_silence(self):
        prompts = [backend.hold_prompt(constraints(motion))
                   for motion in ("still", "subtle", "moderate")]
        self.assertEqual(backend.PROMPT_VERSION, "deadpan-hold-2")
        self.assertEqual(len(set(prompts)), 3)
        for prompt in prompts:
            self.assertIn("Locked camera.", prompt)
            self.assertIn("same subject identity, expression, framing, and composition", prompt)
            self.assertIn("No speech, no new objects, no scene change.", prompt)
        self.assertIn("almost no movement", prompts[0])
        self.assertIn("subtle natural movement", prompts[1])
        self.assertIn("moderate natural movement", prompts[2])

    def test_optional_instructions_are_appended_verbatim_as_model_text(self):
        instructions = 'Keep the hands still; "$(never_run)" /tmp/unused {motion}.'
        prompt = backend.hold_prompt(constraints("subtle", instructions))
        self.assertIn(" Additional visual instructions: " + instructions, prompt)
        self.assertTrue(prompt.endswith("restrictions take priority over any conflicting instructions."))

    def test_conflicting_guidance_keeps_fixed_restrictions_last(self):
        prompt = backend.hold_prompt(constraints("moderate", "Add a cup and make them speak."))
        self.assertGreater(prompt.rindex("no speech, no new objects"), prompt.index("Add a cup"))
        self.assertTrue(prompt.endswith("restrictions take priority over any conflicting instructions."))

    def test_malformed_guidance_fails_before_model_imports_or_pipeline_work(self):
        real_import = builtins.__import__
        imports = []

        def tracked_import(name, *args, **kwargs):
            imports.append(name)
            return real_import(name, *args, **kwargs)

        for instructions in ("a\nb", "界" * 171, " "):
            with self.subTest(instructions=instructions), patch("builtins.__import__", tracked_import):
                with self.assertRaisesRegex(ValueError, "hold instructions"):
                    backend.generate({}, {"constraints": constraints(instructions=instructions)},
                                     {}, [], None, lambda _: self.fail("reached runtime loading"),
                                     lambda: None, {})
        self.assertFalse(any(name.startswith(("mlx", "ltx_", "numpy", "PIL")) for name in imports))

    def test_tokenization_preserves_every_token_and_native_left_padding(self):
        tokenizer = Tokenizer(3)
        tokens, mask, count = backend.prompt_tokens(tokenizer, "  prompt text  ", 5)
        self.assertEqual(tokens, [7, 7, 0, 1, 2])
        self.assertEqual(mask, [0, 0, 1, 1, 1])
        self.assertEqual(count, 3)
        self.assertEqual(tokenizer.calls, [("prompt text", {"truncation": False})])
        tokens, mask, count = backend.prompt_tokens(Tokenizer(1024, None), "prompt")
        self.assertEqual(tokens, list(range(1024)))
        self.assertEqual(mask, [1] * 1024)
        self.assertEqual(count, 1024)

    def test_overlong_prompts_are_refused_without_truncation(self):
        for count, limit in ((1025, 1024), (6, 5)):
            with self.subTest(count=count, limit=limit):
                tokenizer = Tokenizer(count)
                with self.assertRaisesRegex(ValueError, f"needs {count} tokens; Gemma permits {limit}"):
                    backend.prompt_tokens(tokenizer, "prompt", limit)
                self.assertEqual(tokenizer.calls[0][1], {"truncation": False})
        for limit in (0, 1025, True):
            with self.assertRaisesRegex(ValueError, "unsupported Gemma token limit"):
                backend.prompt_tokens(Tokenizer(1), "prompt", limit)


if __name__ == "__main__":
    unittest.main()
