#!/usr/bin/env python3
"""Keep documents that two skills must both carry identical.

A skill is installed on its own, so arca-admin cannot link to files inside arca-user. The source
of truth is the first path; the second is a generated copy.

    python3 skills/sync_shared.py          # rewrite the copies
    python3 skills/sync_shared.py --check  # exit 1 if a copy is stale
"""
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
SHARED = [
    ('arca-user/references/client-setup.md', 'arca-admin/references/product/client-setup.md'),
]


def expected(source):
    text = (HERE / source).read_text()
    return f'<!-- Synced from skills/{source} by skills/sync_shared.py. Edit the source, not this copy. -->\n\n{text}'


def main():
    stale = []
    for source, copy in SHARED:
        want, target = expected(source), HERE / copy
        if target.exists() and target.read_text() == want:
            continue
        stale.append(copy)
        if '--check' not in sys.argv:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(want)
    if '--check' in sys.argv:
        for copy in stale:
            print('FAIL stale copy:', copy)
        return 1 if stale else 0
    print('synced:', ', '.join(stale) or 'nothing to do')
    return 0


if __name__ == '__main__':
    sys.exit(main())
