"""Landing-only regression checks: python tools/check-landing-motion.py.

Uses the standard library; does not build, install, or run the audio application.
"""
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
MOTION = '  .rise{opacity:0;transform:translateY(18px);animation:rise .7s cubic-bezier(.2,.7,.2,1) forwards}\n'
KEYFRAMES = '  @keyframes rise{to{opacity:1;transform:none}}\n'
REDUCED = '  @media (prefers-reduced-motion:reduce){.rise{opacity:1;transform:none}}\n'
DISABLE = '@media (prefers-reduced-motion:reduce){*{animation:none!important;transition:none!important;scroll-behavior:auto}}'


def check(source):
    """Keep visible reduced-motion content after the normal rise declaration."""
    style = source.split('<style>', 1)[1].split('</style>', 1)[0]
    assert style.count(MOTION) == 1
    assert style.count(KEYFRAMES) == 1
    assert style.count(REDUCED) == 1
    assert DISABLE in style
    assert style.endswith(MOTION + KEYFRAMES + REDUCED)


class LandingMotion(unittest.TestCase):
    def setUp(self):
        self.source = (ROOT / 'docs/index.html').read_text(encoding='utf-8')

    def test_visible_content_without_animation(self):
        check(self.source)

    def test_missing_visibility_rule_is_rejected(self):
        with self.assertRaises(AssertionError):
            check(self.source.replace(REDUCED, ''))

    def test_rule_before_base_declaration_is_rejected(self):
        misplaced = self.source.replace(REDUCED, '').replace(MOTION, REDUCED + MOTION)
        with self.assertRaises(AssertionError):
            check(misplaced)

    def test_duplicate_or_incomplete_rule_is_rejected(self):
        for changed in (REDUCED + REDUCED, REDUCED.replace('opacity:1', 'opacity:0'),
                        REDUCED.replace('transform:none', 'transform:translateY(18px)')):
            with self.subTest(rule=changed), self.assertRaises(AssertionError):
                check(self.source.replace(REDUCED, changed))


if __name__ == '__main__':
    unittest.main()
