"""Published CIEDE2000 edge cases; no images or expensive sweeps."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('chart', Path(__file__).parents[1] / 'profile_chart_deltae.py')
chart = importlib.util.module_from_spec(spec)
spec.loader.exec_module(chart)


class ChartMetricTests(unittest.TestCase):
    def test_ciede2000_published_sharma_pairs_and_symmetry(self):
        # Sharma, Wu, Dalal supplementary test data (pairs 1, 7, 9, 10).
        pairs = [((50, 2.6772, -79.7751), (50, 0, -82.7485), 2.0425),
                 ((50, 0, 0), (50, -1, 2), 2.3669),
                 ((50, 2.49, -0.001), (50, -2.49, 0.0009), 7.1792),
                 ((50, 2.49, -0.001), (50, -2.49, 0.001), 7.1792)]
        for p, q, expected in pairs:
            self.assertAlmostEqual(chart.delta_e_2000(p, q), expected, places=4)
            self.assertAlmostEqual(chart.delta_e_2000(q, p), expected, places=4)
            self.assertEqual(chart.delta_e_2000(p, p), 0)


if __name__ == '__main__':
    unittest.main()
