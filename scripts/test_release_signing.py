#!/usr/bin/env python3
"""GnuPG integration tests: only ephemeral keys, never release-key secrets."""
from contextlib import ExitStack
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

import release_signing as rs


class CommittedKey(unittest.TestCase):
    def test_committed_public_key_has_the_pinned_primary_fingerprint(self):
        self.assertEqual(rs.PRIMARY_FINGERPRINT, 'B281C3010A76A11737B4D066C7396F6EA7647CC6')
        self.assertEqual(rs.SIGNING_SUBKEY, '248DFD639B75D1EFF2A87289D7C7DE7BBCCF35EC')
        with rs.temporary_home() as home:
            rs._gpg(home, '--import', input=rs.PUBLIC_KEY.read_bytes())
            self.assertEqual(rs.primary_fingerprints(home), [rs.PRIMARY_FINGERPRINT])
        self.assertFalse(home.exists())


class ThrowawayKey(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.resources = ExitStack()
        cls.addClassCleanup(cls.resources.close)
        cls.home = cls.resources.enter_context(rs.temporary_home())
        cls.passphrase = b'throwaway-test-passphrase'
        rs._gpg(cls.home, '--pinentry-mode', 'loopback', '--passphrase-fd', '0',
                '--quick-generate-key', 'Release signing test <test@example.invalid>',
                'ed25519', 'cert', '1d', input=cls.passphrase + b'\n')
        cls.primary = rs.primary_fingerprints(cls.home)[0]
        rs._gpg(cls.home, '--pinentry-mode', 'loopback', '--passphrase-fd', '0',
                '--quick-add-key', cls.primary, 'ed25519', 'sign', '1d', input=cls.passphrase + b'\n')
        fingerprints = [line.split(':')[9] for line in
                        rs._gpg(cls.home, '--with-colons', '--fingerprint', '--list-keys').decode().splitlines()
                        if line.startswith('fpr:')]
        cls.subkey = fingerprints[-1]
        cls.public = rs._gpg(cls.home, '--armor', '--export', cls.primary)

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)
        self.manifest = self.folder / 'SHA256SUMS'
        self.signature = self.folder / 'SHA256SUMS.asc'
        self.manifest.write_text('test checksum manifest\n')

    def sign(self):
        rs._sign(self.manifest, self.signature, self.home, self.subkey, self.passphrase)

    def test_passphrase_is_stdin_not_a_command_line_argument(self):
        with mock.patch.object(rs, '_gpg') as gpg:
            rs._sign(self.manifest, self.signature, self.home, self.subkey, self.passphrase)
        args = gpg.call_args.args
        self.assertEqual(args[args.index('--passphrase-fd') + 1], '0')
        self.assertEqual(args[args.index('--local-user') + 1], self.subkey + '!')
        self.assertFalse(any(self.passphrase.decode() in str(arg) for arg in args))
        self.assertEqual(gpg.call_args.kwargs['input'], self.passphrase + b'\n')

    def test_sign_and_verify_with_separate_public_only_home(self):
        self.sign()
        self.assertTrue(self.signature.read_text().startswith('-----BEGIN PGP SIGNATURE-----'))
        with rs.temporary_home() as verifier:
            rs._verify(self.manifest, self.signature, verifier, self.public, self.primary)
            secret = rs._gpg(verifier, '--with-colons', '--list-secret-keys')
            self.assertNotIn(b'sec:', secret)
            status = rs._gpg(verifier, '--status-fd', '1', '--verify', str(self.signature), str(self.manifest))
            self.assertIn(self.subkey.encode(), status)
            self.assertIn(self.primary.encode(), status)
        self.assertFalse(verifier.exists())

    def test_tampering_with_manifest_or_signature_is_rejected(self):
        self.sign()
        original = self.manifest.read_bytes()
        self.manifest.write_bytes(original + b'changed\n')
        with rs.temporary_home() as home, self.assertRaises(ValueError):
            rs._verify(self.manifest, self.signature, home, self.public, self.primary)
        self.manifest.write_bytes(original)
        self.signature.write_text('not a signature')
        with rs.temporary_home() as home, self.assertRaises(ValueError):
            rs._verify(self.manifest, self.signature, home, self.public, self.primary)

    def test_wrong_primary_pin_is_rejected_even_with_a_valid_signature(self):
        self.sign()
        with rs.temporary_home() as home, self.assertRaisesRegex(ValueError, 'primary fingerprint'):
            rs._verify(self.manifest, self.signature, home, self.public, rs.PRIMARY_FINGERPRINT)

    def test_production_cli_cannot_use_a_throwaway_key_or_override_the_pin(self):
        self.sign()
        env = dict(os.environ, RELEASE_SIGNING_PRIMARY_FINGERPRINT=self.primary,
                   RELEASE_SIGNING_PUBLIC_KEY=self.public.decode())
        result = subprocess.run([sys.executable, str(Path(rs.__file__)), 'verify', str(self.folder)],
                                env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        result = subprocess.run([sys.executable, str(Path(rs.__file__)), 'verify', str(self.folder),
                                 '--expected-fingerprint', self.primary], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('unrecognized arguments', result.stderr)

    def test_validsig_requires_the_primary_not_just_the_signing_fingerprint(self):
        self.sign()
        with rs.temporary_home() as home:
            rs._import_public(home, self.public, self.primary)
            status = rs._gpg(home, '--status-fd', '1', '--verify', str(self.signature), str(self.manifest))
        rs._require_validsig(status, self.primary)
        for bad in (b'', status.replace(self.primary.encode(), b'0' * 40), status + status):
            with self.assertRaises(ValueError):
                rs._require_validsig(bad, self.primary)
        with self.assertRaises(ValueError):
            rs._require_validsig(status, self.subkey)

    def test_production_signer_is_fixed_to_the_release_subkey_and_verifier(self):
        # Exercise production orchestration with ephemeral primitives; the CLI
        # exposes none of these mocks or test-key parameters.
        with rs.temporary_home() as home, rs.temporary_home() as verifier:
            env = dict(GNUPGHOME=str(home), RELEASE_SIGNING_KEY='throwaway key',
                       RELEASE_SIGNING_PASSPHRASE='throwaway passphrase')
            def sign(*args): self.signature.write_text('test signature')
            with mock.patch.dict(os.environ, env), mock.patch.object(rs, '_gpg') as gpg, \
                    mock.patch.object(rs, '_sign', side_effect=sign) as signing, \
                    mock.patch.object(rs, '_verify') as verifying, mock.patch('builtins.print'):
                self.assertNotIn('RELEASE_SIGNING_PASSPHRASE', rs._env(home))
                self.assertNotIn('RELEASE_SIGNING_KEY', rs._env(home))
                rs.sign(self.folder, verifier)
            self.assertEqual(gpg.call_args.kwargs['input'], b'throwaway key')
            self.assertEqual(signing.call_args.args[3:], (rs.SIGNING_SUBKEY, b'throwaway passphrase'))
            self.assertEqual(verifying.call_args.args[3:], (rs.PUBLIC_KEY.read_bytes(), rs.PRIMARY_FINGERPRINT))
            self.assertNotIn('RELEASE_SIGNING_PASSPHRASE', rs._env(home))
            self.assertNotIn('RELEASE_SIGNING_KEY', rs._env(home))

    def test_failed_signing_or_verification_removes_candidate_signature(self):
        for operation in ('_sign', '_verify'):
            with self.subTest(operation=operation), rs.temporary_home() as home, rs.temporary_home() as verifier:
                def fail(*args):
                    self.signature.write_text('invalid candidate')
                    raise ValueError('deliberate test failure')
                with mock.patch.dict(os.environ, GNUPGHOME=str(home), RELEASE_SIGNING_KEY='throwaway',
                                     RELEASE_SIGNING_PASSPHRASE='throwaway'), \
                        mock.patch.object(rs, '_gpg'), mock.patch.object(rs, '_sign'), \
                        mock.patch.object(rs, '_verify'), mock.patch.object(rs, operation, side_effect=fail):
                    with self.assertRaises(ValueError): rs.sign(self.folder, verifier)
                self.assertFalse(self.signature.exists())


class KeyLifecycle(unittest.TestCase):
    def test_revoked_subkey_is_refused_even_with_validsig(self):
        with rs.temporary_home() as signer, tempfile.TemporaryDirectory() as folder:
            manifest, signature = Path(folder) / 'SHA256SUMS', Path(folder) / 'SHA256SUMS.asc'
            manifest.write_text('throwaway manifest\n')
            rs._gpg(signer, '--pinentry-mode', 'loopback', '--passphrase', '',
                    '--quick-generate-key', 'Revocation test <test@example.invalid>', 'ed25519', 'cert', '0')
            primary = rs.primary_fingerprints(signer)[0]
            rs._gpg(signer, '--pinentry-mode', 'loopback', '--passphrase', '',
                    '--quick-add-key', primary, 'ed25519', 'sign', '0')
            subkey = [line.split(b':')[9].decode() for line in
                      rs._gpg(signer, '--with-colons', '--fingerprint', '--list-keys').splitlines()
                      if line.startswith(b'fpr:')][-1]
            rs._sign(manifest, signature, signer, subkey, b'')
            rs._gpg(signer, '--pinentry-mode', 'loopback', '--command-fd', '0', '--edit-key', primary,
                    input=b'key 1\nrevkey\ny\n0\n\ny\nsave\n')
            public = rs._gpg(signer, '--armor', '--export', primary)
            with rs.temporary_home() as verifier:
                rs._import_public(verifier, public, primary)
                status = rs._gpg(verifier, '--status-fd', '1', '--verify', str(signature), str(manifest))
                self.assertIn(b'VALIDSIG', status)
                self.assertIn(b'REVKEYSIG', status)
                with self.assertRaisesRegex(ValueError, 'revoked'):
                    rs._require_validsig(status, primary)
            with rs.temporary_home() as verifier, self.assertRaisesRegex(ValueError, 'revoked'):
                rs._verify(manifest, signature, verifier, public, primary)

    def test_expired_subkey_accepts_only_a_signature_before_expiry(self):
        created = int(time.time()) - 3 * 86400
        clock = ('--faked-system-time', str(created) + '!')
        with rs.temporary_home() as signer, tempfile.TemporaryDirectory() as folder:
            manifest, signature = Path(folder) / 'SHA256SUMS', Path(folder) / 'SHA256SUMS.asc'
            manifest.write_text('historical throwaway manifest\n')
            rs._gpg(signer, *clock, '--pinentry-mode', 'loopback', '--passphrase', '',
                    '--quick-generate-key', 'Expiry test <test@example.invalid>', 'ed25519', 'cert', '0')
            primary = rs.primary_fingerprints(signer)[0]
            rs._gpg(signer, *clock, '--pinentry-mode', 'loopback', '--passphrase', '',
                    '--quick-add-key', primary, 'ed25519', 'sign', '1d')
            listing = rs._gpg(signer, '--with-colons', '--fingerprint', '--list-keys')
            subkey = [line.split(b':')[9].decode() for line in listing.splitlines()
                      if line.startswith(b'fpr:')][-1]
            rs._gpg(signer, '--faked-system-time', str(created + 3600) + '!',
                    '--pinentry-mode', 'loopback', '--passphrase-fd', '0', '--local-user', subkey + '!',
                    '--armor', '--detach-sign', '--output', str(signature), str(manifest), input=b'\n')
            public = rs._gpg(signer, '--armor', '--export', primary)
            with rs.temporary_home() as verifier:
                rs._verify(manifest, signature, verifier, public, primary)
                status = rs._gpg(verifier, '--status-fd', '1', '--verify', str(signature), str(manifest))
                self.assertIn(b'EXPKEYSIG', status)
                self.assertIn(b'KEYEXPIRED', status)
                expiry = created + 86400
                for timestamp in (expiry, expiry + 1):
                    altered = b'\n'.join(b' '.join(row.split()[:4] + [str(timestamp).encode()] + row.split()[5:])
                                         if row.startswith(b'[GNUPG:] VALIDSIG ') else row
                                         for row in status.splitlines())
                    with self.assertRaisesRegex(ValueError, 'before signing-key expiry'):
                        rs._require_validsig(altered, primary, listing)
                with self.assertRaises(ValueError):
                    rs._require_validsig(status, primary, b'')

    def test_all_refused_statuses_and_signature_counts(self):
        primary = rs.PRIMARY_FINGERPRINT
        valid = f'[GNUPG:] VALIDSIG {primary} 2026-11-22 1795305600 0 4 0 22 8 00 {primary}\n'.encode()
        good = b'[GNUPG:] GOODSIG test Test key\n'
        rs._require_validsig(good + valid, primary)
        for bad in ('REVKEYSIG', 'KEYREVOKED', 'BADSIG', 'ERRSIG', 'EXPSIG', 'NO_PUBKEY'):
            with self.subTest(status=bad), self.assertRaises(ValueError):
                rs._require_validsig(good + valid + f'[GNUPG:] {bad} test\n'.encode(), primary)
        for status in (valid, good + good + valid, good + valid + valid):
            with self.assertRaises(ValueError):
                rs._require_validsig(status, primary)


class Workflow(unittest.TestCase):
    def test_signing_precedes_token_and_is_not_skipped_for_an_existing_crate(self):
        text = (rs.PUBLIC_KEY.parent / 'workflows' / 'trusted-publishing.yml').read_text()
        signing = text.index('name: Sign and verify the OpenPGP checksum signature')
        token = text.index('uses: rust-lang/crates-io-auth-action@')
        self.assertLess(signing, token)
        publish = text[text.index('\n  publish:\n'):text.index('\n  github-release:\n')]
        before_signing = publish[:publish.index('name: Sign and verify')]
        for line in before_signing.splitlines():
            if 'uses:' in line:
                self.assertTrue(line.split('uses:', 1)[1].strip().startswith('actions/'))
        self.assertIn('rustup toolchain install 1.95.0 --profile minimal', publish)
        step = text[signing:text.index('      - ', signing)]
        self.assertNotIn('if:', step)
        self.assertIn('RELEASE_SIGNING_PASSPHRASE:', step)
        self.assertEqual(text.count('RELEASE_SIGNING_PASSPHRASE:'), 1)
        self.assertLess(text.index('name: Upload the OpenPGP checksum signature'), token)


if __name__ == '__main__':
    unittest.main()
