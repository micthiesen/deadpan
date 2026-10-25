#!/usr/bin/env python3
"""Maintain the small, versioned data file shown by the Deadpan progress artifact."""
import argparse
from datetime import date, datetime, timezone
from fractions import Fraction
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
REPORT = ROOT / 'docs/progress.json'
STAGES = {'planned', 'partial', 'mostly', 'done'}


def assessment_score(assessment):
    """Equal requirement weights; criteria within each requirement share its weight."""
    if not assessment.get('id') or not assessment.get('method'):
        raise ValueError('Assessment identity and method are required')
    rows = assessment['requirements']
    if sorted(row['id'] for row in rows) != [f'DP-{i:02}' for i in range(1, 25)]:
        raise ValueError('Assess each of the 24 requirements exactly once')
    scores = []
    for row in rows:
        criteria = row['criteria']
        if not 3 <= len(criteria) <= 8:
            raise ValueError(f'{row["id"]}: expected 3-8 major acceptance criteria')
        if len({item['id'] for item in criteria}) != len(criteria):
            raise ValueError(f'{row["id"]}: duplicate criterion')
        for item in criteria:
            if type(item['score']) not in (int, float) or item['score'] not in (0, 0.5, 1):
                raise ValueError('Criterion scores must be 0, 0.5 or 1')
            if not item['deliverable'] or not item['evidence']:
                raise ValueError('Each criterion needs a deliverable and evidence')
            if item['score'] < 1 and not item['remaining']:
                raise ValueError('Partial and absent criteria need concrete remaining work')
        scores.append(sum(Fraction(str(item['score'])) for item in criteria) / len(criteria))
    percent = 100 * sum(scores) / len(scores)
    # Round halves upward, independent of floating point and Python's even rounding.
    return (2 * percent.numerator + percent.denominator) // (2 * percent.denominator)


def requirements():
    rows = {}
    for line in (ROOT / 'docs/REQUIREMENTS.md').read_text().splitlines():
        fields = [field.strip() for field in line.split('|')]
        if len(fields) > 4 and re.fullmatch(r'DP-\d{2}', fields[1]):
            rows[fields[1]] = fields[3]
    if len(rows) != 24:
        raise ValueError('Expected all 24 requirement rows in docs/REQUIREMENTS.md')
    return rows


def validate(report, statuses):
    if report.get('schema_version') != 1 or not report.get('history'):
        raise ValueError('Unsupported or empty progress report')
    if not report.get('focus') or not report.get('estimate_note'):
        raise ValueError('Focus and estimate note are required')
    datetime.fromisoformat(report['updated_at'].replace('Z', '+00:00'))
    ids, covered = set(), []
    for feature in report['features']:
        if feature['id'] in ids or feature['status'] not in STAGES:
            raise ValueError('Duplicate feature ID or unknown status')
        ids.add(feature['id'])
        if not feature['name'] or not feature['note'] or not feature['requirements']:
            raise ValueError('Each feature needs a name, note and requirement mapping')
        for item in feature['requirements']:
            if item not in statuses:
                raise ValueError(f'Unknown requirement: {item}')
            if feature['status'] == 'done' and statuses[item] != 'Complete':
                raise ValueError(f'{feature["name"]} cannot be done: {item} is {statuses[item]}')
        covered.extend(feature['requirements'])
    if sorted(covered) != sorted(statuses):
        raise ValueError('Map every requirement to exactly one high-level feature')
    previous = None
    for point in report['history']:
        day = date.fromisoformat(point['date'])
        if previous and day < previous:
            raise ValueError('History must stay in chronological order')
        previous = day
        percent = point['percent']
        if type(percent) is not int or not 0 <= percent <= 100:
            raise ValueError('Percent must be an integer from 0 to 100')
        if not point['summary'] or not re.fullmatch(r'[0-9a-f]{7,40}', point['revision']):
            raise ValueError('Each milestone needs a summary and Git revision')
        if type(point['complete_sections']) is not int or not 0 <= point['complete_sections'] <= 24:
            raise ValueError('Invalid complete-section count')
    current = report['history'][-1]
    assessment = report.get('assessment')
    if assessment is not None:
        score = assessment_score(assessment)
        if current.get('assessment_id') != assessment['id'] or current['percent'] != score:
            raise ValueError('Current percentage must match the identified acceptance ledger')
    complete = sum(value == 'Complete' for value in statuses.values())
    if current['complete_sections'] != complete:
        raise ValueError('Strict completion count changed; record a milestone')
    if current['percent'] == 100 and complete != 24:
        raise ValueError('Do not report 100% while requirement sections remain open')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('check', help='Validate the report against Requirements')
    sub.add_parser('show', help='Print a concise progress summary')
    record = sub.add_parser('record', help='Append a meaningful milestone; never commits or pushes')
    record.add_argument('--percent', type=int, help='Optional assertion; the acceptance ledger computes coverage')
    record.add_argument('--summary', required=True)
    record.add_argument('--focus', required=True)
    record.add_argument('--stage', action='append', default=[], metavar='FEATURE=STATUS')
    record.add_argument('--revision', help='Verified source revision; defaults to HEAD')
    args = parser.parse_args()
    report = json.loads(REPORT.read_text())
    statuses = requirements()
    if args.command == 'record':
        percent = assessment_score(report['assessment'])
        if args.percent is not None and args.percent != percent:
            raise ValueError(f'Ledger computes {percent}%; update criterion evidence before changing progress')
        for change in args.stage:
            identifier, separator, status = change.partition('=')
            feature = next((f for f in report['features'] if f['id'] == identifier), None)
            if not separator or feature is None or status not in STAGES:
                raise ValueError(f'Unknown feature or stage: {change}')
            feature['status'] = status
        now = datetime.now(timezone.utc)
        revision = args.revision or subprocess.check_output(
            ['git', 'rev-parse', '--short', 'HEAD'], cwd=ROOT, text=True).strip()
        report['updated_at'] = now.isoformat(timespec='seconds').replace('+00:00', 'Z')
        report['focus'] = args.focus
        report['history'].append({
            'date': now.date().isoformat(), 'percent': percent,
            'assessment_id': report['assessment']['id'],
            'complete_sections': sum(s == 'Complete' for s in statuses.values()),
            'revision': revision, 'retrospective': False, 'summary': args.summary,
        })
        validate(report, statuses)
        temporary = REPORT.with_suffix('.json.tmp')
        temporary.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
        temporary.replace(REPORT)
    else:
        validate(report, statuses)
    current = report['history'][-1]
    print(f"~{current['percent']}% assessed requirement coverage; not time remaining or release readiness")
    print(f"{current['complete_sections']}/24 requirement sections complete; updated {report['updated_at']}")
    print(report['focus'])


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f'Progress update failed: {error}', file=sys.stderr)
        sys.exit(1)
