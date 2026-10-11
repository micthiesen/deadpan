from pathlib import Path
import json

root = Path(__file__).parent
for folder in ['replays', 'replays-final', 'replays-final-2', 'replays-final-3']:
    for source in sorted((root/folder).glob('*/report.json')):
        report = json.loads(source.read_text())
        print(source.parent.name, folder, 'failed:', report['failed'], 'binary:', report['metadata']['binary_sha256'])
        for scenario in report['scenarios']:
            print(' ', scenario['name'], 'checks:', len(scenario['checks']), 'failed:', scenario['failed'])
            for finding in scenario['findings']:
                print(' ', finding['severity'], finding['message'][:500])
            for step in scenario['steps']:
                if step.get('semantic', {}).get('checkpoint') in ['Selected sound repaired', 'Missing sound report']:
                    print(' ', step['semantic']['checkpoint'], source.parent/step['screenshot'])
