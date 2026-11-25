#!/usr/bin/env python3
"""Tests for release_assets.py (standard library only).

Run: python3 -m unittest discover -s scripts -p 'test_*.py'
"""
import base64
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import subprocess

sys.path.insert(0, str(Path(__file__).resolve().parent))
import release_assets as ra  # noqa: E402

N = 'rust-mirror-gui'


def archives(v):
    return sorted(f'{N}-{v}-{t}' for t in ('x86_64-pc-windows-msvc.zip', 'x86_64-unknown-linux-gnu.tar.gz',
                                         'aarch64-apple-darwin.tar.gz', 'x86_64-apple-darwin.tar.gz'))


class Layouts(unittest.TestCase):
    def test_043_and_050_keep_four_archives_signed_by_release_binaries(self):
        for v in ('0.4.3', '0.5.0'):
            lay = ra.layout(v)
            self.assertEqual(lay['summed'], archives(v))
            self.assertEqual(lay['assets'], sorted(archives(v) + ['SHA256SUMS']))
            self.assertEqual((lay['signer'], lay['strict'], lay['sbom'], lay['bundle'], lay['crate']),
                             ('release-binaries.yml', False, None, None, None))
            self.assertEqual(lay['sbom_subjects'], [])

    def test_051_adds_the_sbom_and_the_l3_signer(self):
        lay = ra.layout('0.5.1')
        self.assertEqual(lay['summed'], sorted(archives('0.5.1') + [f'{N}-0.5.1.cdx.json']))
        self.assertEqual(len(lay['assets']), 6)
        self.assertEqual((lay['signer'], lay['strict'], lay['bundle'], lay['crate']), ('build-binaries.yml', True, None, None))
        self.assertEqual(lay['sbom_subjects'], archives('0.5.1'))

    def test_052_adds_the_bundle_only(self):
        lay = ra.layout('0.5.2')
        self.assertEqual(lay['assets'], sorted(archives('0.5.2') + [f'{N}-0.5.2.cdx.json', 'SHA256SUMS', f'{N}-0.5.2.intoto.jsonl']))
        self.assertIsNone(lay['crate'])
        self.assertEqual(lay['signed'], lay['summed'] + ['SHA256SUMS'])

    def test_after_052_one_sha256sums_also_lists_the_crate(self):
        for v in ('0.5.3', '0.5.4'):
            lay = ra.layout(v)
            self.assertEqual(lay['summed'], sorted(archives(v) + [f'{N}-{v}.cdx.json', f'{N}-{v}.crate']))
            self.assertEqual(lay['assets'], sorted(lay['summed'] + ['SHA256SUMS', f'{N}-{v}.intoto.jsonl']))
            self.assertEqual(len(lay['assets']), 8)
            self.assertEqual(lay['sbom_subjects'], sorted(archives(v) + [f'{N}-{v}.crate']))
            self.assertEqual((lay['signer'], lay['strict']), ('build-binaries.yml', True))

    def test_after_054_adds_only_the_openpgp_signature(self):
        for v in ('0.5.5', '0.6.0', '1.0.0'):
            lay = ra.layout(v)
            self.assertEqual(lay['openpgp'], 'SHA256SUMS.asc')
            self.assertEqual(len(lay['assets']), 9)
            self.assertEqual(len(lay['summed']), 6)
            self.assertEqual(len(lay['signed']), 7)
            self.assertIn('SHA256SUMS.asc', lay['assets'])
            self.assertNotIn('SHA256SUMS.asc', lay['signed'])
            self.assertNotIn('SHA256SUMS.asc', lay['summed'])
            with self.assertRaises(ValueError):
                ra.check_names(v, [n for n in lay['assets'] if n != 'SHA256SUMS.asc'])
        for v in ('0.4.3', '0.5.0', '0.5.1', '0.5.2', '0.5.3', '0.5.4'):
            lay = ra.layout(v)
            self.assertIsNone(lay['openpgp'])
            with self.assertRaises(ValueError):
                ra.check_names(v, lay['assets'] + ['SHA256SUMS.asc'])

    def test_sha256sums_order_is_byte_order(self):
        lay = ra.layout('0.5.3')
        self.assertEqual(lay['summed'][-2:], [f'{N}-0.5.3.cdx.json', f'{N}-0.5.3.crate'])

    def test_builds_always_have_the_newest_layout(self):
        # A dry run of an already-released Cargo.toml version (main at 0.5.2) builds the new profile.
        for v in ('0.5.2', '0.4.0', '0.5.3'):
            lay = ra.layout(v, build=True)
            self.assertEqual(lay['summed'], sorted(archives(v) + [f'{N}-{v}.cdx.json', f'{N}-{v}.crate']))
            self.assertEqual(len(lay['assets']), 9)
        self.assertEqual(ra.layout('0.5.5', build=True), ra.layout('0.5.5'))
        self.assertNotEqual(ra.layout('0.5.2', build=True), ra.layout('0.5.2'))
        self.assertNotEqual(ra.layout('0.5.4', build=True), ra.layout('0.5.4'))

    def test_no_downloads_before_043_and_no_odd_versions(self):
        for v in ('0.4.2', '0.1.0', 'v0.5.3', '0.5', '0.5.3-rc.1'):
            with self.assertRaises(ValueError):
                ra.layout(v)

    def test_exact_asset_sets(self):
        old = ra.layout('0.5.2')['assets']
        ra.check_names('0.5.2', old)
        for bad in (old + [f'{N}-0.5.2.crate'], old + ['SHA256SUMS-crates'], old[1:]):
            with self.assertRaises(ValueError):
                ra.check_names('0.5.2', bad)
        new = ra.layout('0.5.3')['assets']
        ra.check_names('0.5.3', new)
        for bad in ([n for n in new if not n.endswith('.crate')], new + [f'{N}-0.5.3.crates.cdx.json'],
                    new + ['SHA256SUMS-crates'], [n for n in new if not n.endswith('.intoto.jsonl')]):
            with self.assertRaises(ValueError):
                ra.check_names('0.5.3', bad)
        with self.assertRaises(ValueError):  # the old layout is not accepted for a new version
            ra.check_names('0.5.3', [n.replace('0.5.2', '0.5.3') for n in old])


def fake_files(folder, names):
    for n in names:
        (folder / n).write_bytes(('content of ' + n).encode())


def placeholder_bundle(folder, names):
    subjects = [{'name': n, 'digest': {'sha256': hashlib.sha256((folder / n).read_bytes()).hexdigest()}} for n in names]
    statement = {'_type': 'https://in-toto.io/Statement/v1', 'predicateType': ra.SLSA, 'subject': subjects, 'predicate': {}}
    return {'verificationMaterial': {'placeholder': True},
            'dsseEnvelope': {'payloadType': 'application/vnd.in-toto+json', 'signatures': [{'sig': 'eA=='}],
                             'payload': base64.b64encode(json.dumps(statement).encode()).decode()}}


class Files(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())

    def test_sums_writes_byte_sorted_manifest_for_the_new_layout(self):
        lay = ra.layout('0.5.3')
        fake_files(self.dir, lay['summed'])
        text = ra.write_sums(self.dir, '0.5.3')
        self.assertEqual([l.split('  ')[1] for l in text.splitlines()], lay['summed'])
        ra.check_build(self.dir, '0.5.3')
        (self.dir / lay['crate']).write_bytes(b'changed')
        with self.assertRaises(ValueError):
            ra.check_build(self.dir, '0.5.3')

    def test_sums_for_a_dry_run_of_a_released_version(self):
        lay = ra.layout('0.5.2', build=True)
        fake_files(self.dir, lay['summed'])
        ra.write_sums(self.dir, '0.5.2')
        ra.check_build(self.dir, '0.5.2')

    def test_sums_refuses_a_missing_crate_or_an_extra_file(self):
        lay = ra.layout('0.5.3')
        fake_files(self.dir, [n for n in lay['summed'] if not n.endswith('.crate')])
        with self.assertRaises(ValueError):
            ra.write_sums(self.dir, '0.5.3')
        fake_files(self.dir, [lay['crate'], 'notes.txt'])
        with self.assertRaises(ValueError):
            ra.write_sums(self.dir, '0.5.3')

    def test_stage_writes_a_bundle_whose_subjects_are_the_signed_files(self):
        lay = ra.layout('0.5.3')
        fake_files(self.dir, lay['summed'])
        ra.write_sums(self.dir, '0.5.3')
        raw = Path(tempfile.mkdtemp()) / 'bundle.json'
        raw.write_text(json.dumps(placeholder_bundle(self.dir, lay['signed'])))
        out = Path(tempfile.mkdtemp())
        self.assertEqual(ra.stage(self.dir, '0.5.3', out, [raw]).name, lay['bundle'])
        short = Path(tempfile.mkdtemp()) / 'short.json'  # SHA256SUMS not covered
        short.write_text(json.dumps(placeholder_bundle(self.dir, lay['summed'])))
        with self.assertRaises(SystemExit):
            ra.stage(self.dir, '0.5.3', Path(tempfile.mkdtemp()), [short])


class OpenPGPVerification(unittest.TestCase):
    def test_earlier_releases_do_not_call_gpg_or_change_checksum_checks(self):
        with mock.patch.object(ra.release_signing, 'verify') as verify, mock.patch.object(ra.subprocess, 'run') as run:
            for version in ('0.4.3', '0.5.0', '0.5.1', '0.5.2', '0.5.3', '0.5.4'):
                ra.verify_openpgp(Path('unused'), ra.layout(version))
            verify.assert_not_called()
            run.assert_not_called()

    def test_new_releases_verify_signature_and_check_every_listed_payload(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            lay = ra.layout('0.5.5')
            fake_files(folder, lay['summed'])
            ra.write_sums(folder, '0.5.5')
            with mock.patch.object(ra.release_signing, 'verify') as verify:
                ra.verify_openpgp(folder, lay)
                verify.assert_called_once_with(folder)
                (folder / lay['summed'][-1]).unlink()
                with self.assertRaises(subprocess.CalledProcessError):
                    ra.verify_openpgp(folder, lay)


class Api(unittest.TestCase):
    def test_only_https_to_github_api_hosts(self):
        for url in ('http://api.github.com/x', 'https://example.com/x', 'https://api.github.com.evil.test/x',
                    'https://user:pw@api.github.com/x', 'https://api.github.com:8443/x', 'file:///etc/passwd',
                    'https://objects.githubusercontent.com/x'):
            with self.assertRaisesRegex(ValueError, 'non-GitHub'):
                ra.api(url)


class FakeGitHub:
    """Enough of the releases API for publish_release."""

    def __init__(self, releases=()):
        self.releases = [dict(r) for r in releases]
        self.calls = []
        self.others = []

    def listing(self):
        return self.releases + self.others

    def __call__(self, url, method='GET', data=None, content_type=None):
        self.calls.append((method, url))
        base = f'https://api.github.com/repos/{ra.REPO}/releases'
        if method == 'GET' and url.startswith(base + '?'):
            return self.listing()
        if method == 'POST' and url == base:
            body = json.loads(data)
            release = dict(id=7, tag_name=body['tag_name'], name=body['name'], body=body['body'], draft=True,
                           prerelease=False, assets=[])
            self.releases.append(release)
            return release
        if method == 'POST' and url.startswith('https://uploads.github.com/'):
            name = url.split('name=')[1]
            asset = dict(name=name, size=len(data), digest='sha256:' + hashlib.sha256(data).hexdigest(), state='uploaded')
            self.releases[0]['assets'].append(asset)
            return asset
        if method == 'PATCH':
            self.patch = json.loads(data)
            self.releases[0]['draft'] = self.patch['draft']
            return self.releases[0]
        if method == 'GET':
            return self.releases[0]
        raise AssertionError((method, url))


class Publish(unittest.TestCase):
    TAG = 'v0.5.3'
    NOTES = '# rust-mirror-gui 0.5.3: notes\n\nBody.\n'

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        fake_files(self.dir, ra.layout('0.5.3')['assets'])

    def asset(self, name):
        raw = (self.dir / name).read_bytes()
        return dict(name=name, size=len(raw), digest='sha256:' + hashlib.sha256(raw).hexdigest(), state='uploaded')

    def release(self, **kw):
        r = dict(id=7, tag_name=self.TAG, name='rust-mirror-gui 0.5.3: notes', body=self.NOTES, draft=True,
                 prerelease=False, assets=[])
        r.update(kw)
        return r

    def test_creates_a_draft_uploads_everything_then_publishes(self):
        gh = FakeGitHub()
        result = ra.publish_release(self.TAG, self.NOTES, self.dir, gh)
        self.assertFalse(result['draft'])
        self.assertEqual(sorted(a['name'] for a in result['assets']), ra.layout('0.5.3')['assets'])
        methods = [m for m, _ in gh.calls]
        self.assertLess(methods.index('POST'), methods.index('PATCH'))
        self.assertEqual(methods.count('PATCH'), 1)

    def test_latest_only_for_the_highest_version(self):
        gh = FakeGitHub()
        gh.others = [dict(tag_name='v0.5.2', draft=False, prerelease=False), dict(tag_name='v0.6.0', draft=True, prerelease=False),
                     dict(tag_name='v0.7.0-rc.1', draft=False, prerelease=True)]
        ra.publish_release(self.TAG, self.NOTES, self.dir, gh)
        self.assertEqual(gh.patch, {'draft': False, 'make_latest': 'true'})
        gh = FakeGitHub()
        gh.others = [dict(tag_name='v0.6.0', draft=False, prerelease=False)]
        ra.publish_release(self.TAG, self.NOTES, self.dir, gh)
        self.assertEqual(gh.patch, {'draft': False, 'make_latest': 'false'})

    def test_new_layout_uploads_nine_assets_and_never_replaces_a_signature(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            version, tag = '0.5.5', 'v0.5.5'
            lay = ra.layout(version)
            fake_files(folder, lay['assets'])
            notes = '# rust-mirror-gui 0.5.5: notes\n'
            gh = FakeGitHub()
            release = ra.publish_release(tag, notes, folder, gh)
            self.assertEqual(len(release['assets']), 9)
            signature = next(a for a in release['assets'] if a['name'] == 'SHA256SUMS.asc')
            signature['digest'] = 'sha256:' + '0' * 64
            gh.calls.clear()
            with self.assertRaisesRegex(ValueError, 'never replacing'):
                ra.publish_release(tag, notes, folder, gh)
            self.assertEqual({method for method, _ in gh.calls}, {'GET'})

    def test_is_highest(self):
        rel = lambda t, **k: {'tag_name': t, 'draft': False, 'prerelease': False, **k}
        self.assertTrue(ra.is_highest('v0.5.3', [rel('v0.5.2'), rel('v0.4.10')]))
        self.assertTrue(ra.is_highest('v0.10.0', [rel('v0.9.9')]))  # numeric, not string order
        self.assertFalse(ra.is_highest('v0.5.3', [rel('v0.5.10')]))
        self.assertFalse(ra.is_highest('v0.5.3', [rel('v0.5.3')]))
        self.assertTrue(ra.is_highest('v0.5.3', [rel('v0.6.0', draft=True), rel('v1.0.0-rc.1', prerelease=True)]))

    def test_resumes_a_draft_and_uploads_only_missing_assets(self):
        first = ra.layout('0.5.3')['assets'][0]
        gh = FakeGitHub([self.release(assets=[self.asset(first)])])
        ra.publish_release(self.TAG, self.NOTES, self.dir, gh)
        uploads = [u for m, u in gh.calls if u.startswith('https://uploads.github.com/')]
        self.assertEqual(len(uploads), 7)
        self.assertFalse(any(first in u for u in uploads))

    def test_refuses_a_half_uploaded_or_different_existing_asset(self):
        first = ra.layout('0.5.3')['assets'][0]
        for bad in (dict(self.asset(first), state='starter'), dict(self.asset(first), digest='sha256:' + '0' * 64),
                    dict(self.asset(first), size=1)):
            gh = FakeGitHub([self.release(assets=[bad])])
            with self.assertRaisesRegex(ValueError, 'never replacing'):
                ra.publish_release(self.TAG, self.NOTES, self.dir, gh)
            self.assertFalse(any(m in ('PATCH',) or u.startswith('https://uploads') for m, u in gh.calls))

    def test_refuses_an_unexpected_asset_or_a_foreign_release(self):
        cases = [self.release(assets=[dict(name='SHA256SUMS-crates', size=1, digest='sha256:x', state='uploaded')]),
                 self.release(name='another title'), self.release(body='edited'), self.release(prerelease=True)]
        for release in cases:
            with self.assertRaises(ValueError):
                ra.publish_release(self.TAG, self.NOTES, self.dir, FakeGitHub([release]))

    def test_a_published_release_is_never_modified(self):
        names = ra.layout('0.5.3')['assets']
        complete = FakeGitHub([self.release(draft=False, assets=[self.asset(n) for n in names])])
        ra.publish_release(self.TAG, self.NOTES, self.dir, complete)
        self.assertEqual({m for m, _ in complete.calls}, {'GET'})
        partial = FakeGitHub([self.release(draft=False, assets=[self.asset(n) for n in names[:3]])])
        with self.assertRaisesRegex(ValueError, 'published release is incomplete'):
            ra.publish_release(self.TAG, self.NOTES, self.dir, partial)
        self.assertEqual({m for m, _ in partial.calls}, {'GET'})

    def test_two_releases_for_one_tag_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'several'):
            ra.publish_release(self.TAG, self.NOTES, self.dir, FakeGitHub([self.release(), self.release(id=8)]))


class Verify(unittest.TestCase):
    def live(self, version, names, **kw):
        r = dict(tag_name='v' + version, draft=False, prerelease=False,
                 assets=[dict(name=n, state='uploaded', digest=None) for n in names])
        r.update(kw)
        return lambda url, *a, **k: r

    def args(self):
        return type('A', (), dict(tag=None, output=Path(tempfile.mkdtemp()) / 'dl', run_target=None, github_output=None))()

    def test_live_asset_set_must_be_the_versions_layout(self):
        # These fail before anything is downloaded.
        cases = [('0.5.5', [n for n in ra.layout('0.5.5')['assets'] if n != 'SHA256SUMS.asc']),
                 ('0.5.2', ra.layout('0.5.2')['assets'] + [f'{N}-0.5.2.crate']),
                 ('0.5.3', [n.replace('0.5.2', '0.5.3') for n in ra.layout('0.5.2')['assets']]),
                 ('0.5.3', ra.layout('0.5.3')['assets'] + ['SHA256SUMS-crates']),
                 ('0.5.0', ra.layout('0.5.1')['assets'])]
        for version, names in cases:
            args = self.args(); args.tag = 'v' + version
            with self.subTest(version=version), self.assertRaises(ValueError):
                ra.verify_command(args, self.live(version, names))
        args = self.args(); args.tag = 'v0.5.3'
        with self.assertRaisesRegex(ValueError, 'not a published'):
            ra.verify_command(args, self.live('0.5.3', ra.layout('0.5.3')['assets'], draft=True))


class ReadmeCommand(unittest.TestCase):
    """The archive README's verify command must be exactly the verifier's strict policy."""

    def test_package_readme_command_equals_the_verifier_policy(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location('package', Path(__file__).resolve().parent.parent / 'packaging' / 'package.py')
        package = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(package)
        commit = 'a' * 40
        want = ra.policy(ra.layout('0.5.3', build=True), commit)
        for family, archive in (('linux', 'x.tar.gz'), ('macos', 'x.tar.gz'), ('windows', 'x.zip')):
            words = package.verify_command(archive, commit, family).replace('`', ' ').replace('\\', ' ').split()
            self.assertEqual(words[:4], ['gh', 'attestation', 'verify', archive])
            got = words[4:]
            pairs = lambda flags: sorted(zip(*[iter(f for f in flags if f != '--deny-self-hosted-runners')] * 2))
            with self.subTest(family=family):
                self.assertEqual(pairs(got), pairs(want))
                self.assertEqual(got.count('--deny-self-hosted-runners'), want.count('--deny-self-hosted-runners'))
                self.assertEqual(len(got), len(want))


if __name__ == '__main__':
    unittest.main()
