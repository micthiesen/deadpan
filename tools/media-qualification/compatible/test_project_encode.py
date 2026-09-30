import json
from pathlib import Path
import tempfile
import unittest

from qualify_project_encode import admit_file, read_capture


class ProjectCaptureTests(unittest.TestCase):
    def test_capture_requires_exactly_one_of_each_case(self):
        cases = [{'name': name} for name in ['structural', 'nonzero', 'software-two', 'odd', 'marker', 'generated', 'after-cancel']]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'report.json'
            path.write_text(json.dumps({'encoded': {'cases': cases}}))
            self.assertEqual(len(read_capture(path)['encoded']['cases']), 7)
            for invalid in [cases + [cases[0]], cases[:-1], cases[:-1] + [cases[0]]]:
                path.write_text(json.dumps({'encoded': {'cases': invalid}}))
                with self.assertRaises(ValueError):
                    read_capture(path)

    def test_report_extent_is_checked_before_reading(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'oversized.json'
            with path.open('wb') as file:
                file.truncate(32 * 1024 * 1024 + 1)
            with self.assertRaises(ValueError):
                read_capture(path)

    def test_file_admission_rejects_extent_and_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'input'
            path.write_bytes(b'input')
            admit_file(path, 5, 5)
            for maximum, expected in [(4, None), (5, 4)]:
                with self.assertRaises(ValueError):
                    admit_file(path, maximum, expected)
            link = Path(directory) / 'link'
            link.symlink_to(path)
            with self.assertRaises(OSError):
                admit_file(link, 5)


if __name__ == '__main__':
    unittest.main()
