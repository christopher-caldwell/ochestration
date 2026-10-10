"""Ticket acceptance checks: the empty-input case intentionally fails at baseline."""

import unittest

from average import average


class AverageTests(unittest.TestCase):
    def test_empty_input(self):
        self.assertIsNone(average([]))

    def test_positive_numbers(self):
        self.assertEqual(average([2, 4]), 3.0)

    def test_negative_numbers(self):
        self.assertEqual(average([-4, -2]), -3.0)

    def test_zero_mean(self):
        self.assertEqual(average([-1, 1]), 0.0)


if __name__ == "__main__":
    unittest.main()
