#!/usr/bin/env python3
"""GnuPG integration tests: only ephemeral keys, never release-key secrets."""
from contextlib import ExitStack
import os
from pathlib import Path
import subprocess
import sys
import tempfile
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


class Workflow(unittest.TestCase):
    def test_signing_precedes_token_and_is_not_skipped_for_an_existing_crate(self):
        text = (rs.PUBLIC_KEY.parent / 'workflows' / 'trusted-publishing.yml').read_text()
        signing = text.index('name: Sign and verify the OpenPGP checksum signature')
        token = text.index('uses: rust-lang/crates-io-auth-action@')
        self.assertLess(signing, token)
        step = text[signing:text.index('      - ', signing)]
        self.assertNotIn('if:', step)
        self.assertIn('RELEASE_SIGNING_PASSPHRASE:', step)
        self.assertEqual(text.count('RELEASE_SIGNING_PASSPHRASE:'), 1)
        self.assertLess(text.index('name: Upload the OpenPGP checksum signature'), token)


if __name__ == '__main__':
    unittest.main()
