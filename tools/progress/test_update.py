"""Regression checks for progress weighting and invalid criterion credit."""
import copy
import unittest

from update import assessment_score


def ledger():
    return {
        'id': 'test', 'method': 'equal requirement weights',
        'requirements': [
            {'id': f'DP-{n:02}', 'criteria': [
                {'id': f'{n}.{i}', 'deliverable': 'Acceptance', 'score': 1,
                 'evidence': ['retained qualification'], 'remaining': ''}
                for i in range(3)]}
            for n in range(1, 25)]}


class AssessmentTest(unittest.TestCase):
    def test_equal_requirement_weights_despite_different_criterion_counts(self):
        data = ledger()
        data['requirements'][0]['criteria'] += [
            dict(data['requirements'][0]['criteria'][0], id=f'1.extra{i}') for i in range(5)]
        for c in data['requirements'][0]['criteria']:
            c.update(score=0, remaining='Missing')
        self.assertEqual(assessment_score(data), 96)

    def test_partial_credit_and_unscored_gates(self):
        data = ledger()
        for r in data['requirements']:
            for c in r['criteria']:
                c.update(score=0.5, remaining='Incomplete local qualification')
        data['gates'] = [{'score': 0}]
        self.assertEqual(assessment_score(data), 50)

    def test_rejects_missing_requirement_and_unexplained_partial(self):
        data = ledger()
        incomplete = copy.deepcopy(data)
        incomplete['requirements'].pop()
        with self.assertRaises(ValueError):
            assessment_score(incomplete)
        data['requirements'][0]['criteria'][0]['score'] = 0.5
        with self.assertRaises(ValueError):
            assessment_score(data)


if __name__ == '__main__':
    unittest.main()
