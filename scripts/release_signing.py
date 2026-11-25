#!/usr/bin/env python3
"""OpenPGP release checksum signing; GnuPG is supplied by the runner.

Production CLI has no key/fingerprint override or test mode. Signing secrets
are read only by `sign`, and passed to GnuPG on stdin, never argv or files.
"""
from contextlib import contextmanager
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile

PRIMARY_FINGERPRINT = 'B281C3010A76A11737B4D066C7396F6EA7647CC6'
SIGNING_SUBKEY = '248DFD639B75D1EFF2A87289D7C7DE7BBCCF35EC'
PUBLIC_KEY = Path(__file__).resolve().parent.parent / '.github' / 'release-signing-key.asc'


def _env(home):
    env = {k: v for k, v in os.environ.items()
           if k not in ('RELEASE_SIGNING_KEY', 'RELEASE_SIGNING_PASSPHRASE')}
    env['GNUPGHOME'] = str(home)
    return env


def _gpg(home, *args, input=None):
    result = subprocess.run(['gpg', '--batch', '--no-options', '--homedir', str(home),
                             '--no-auto-key-retrieve', *args], input=input, env=_env(home),
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        # Do not include input or GnuPG's import diagnostics: they may describe a secret.
        raise ValueError('GnuPG operation failed')
    return result.stdout


@contextmanager
def temporary_home():
    with tempfile.TemporaryDirectory(prefix='release-gpg-') as folder:
        home = Path(folder)
        home.chmod(0o700)
        try:
            yield home
        finally:
            subprocess.run(['gpgconf', '--kill', 'gpg-agent'], env=_env(home),
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)


def primary_fingerprints(home):
    lines = _gpg(home, '--with-colons', '--fingerprint', '--list-keys').decode().splitlines()
    fingerprints, primary = [], False
    for line in lines:
        fields = line.split(':')
        if fields[0] in ('pub', 'sub'):
            primary = fields[0] == 'pub'
        elif fields[0] == 'fpr' and primary:
            fingerprints.append(fields[9])
            primary = False
    return fingerprints


def _import_public(home, key, expected_primary):
    if any(home.iterdir()):
        raise ValueError('public verification needs a fresh GnuPG home')
    _gpg(home, '--import', input=key)
    if primary_fingerprints(home) != [expected_primary]:
        raise ValueError('unexpected release public-key primary fingerprint')


def _require_validsig(status, expected_primary):
    valid = [line.split() for line in status.splitlines()
             if line.startswith(b'[GNUPG:] VALIDSIG ')]
    # GnuPG appends the primary fingerprint for a subkey signature. For a
    # primary-key signature it may omit that optional field.
    if len(valid) != 1 or len(valid[0]) not in (11, 12):
        raise ValueError('expected exactly one GnuPG VALIDSIG')
    fields = valid[0]
    primary = fields[11] if len(fields) == 12 else fields[2]
    if primary != expected_primary.encode('ascii'):
        raise ValueError('signature primary fingerprint is not the release key')


def _verify(manifest, signature, home, public_key, expected_primary):
    """Low-level primitive also exercised with throwaway keys by unit tests."""
    _import_public(home, public_key, expected_primary)
    status = _gpg(home, '--status-fd', '1', '--verify', str(signature.resolve()), str(manifest.resolve()))
    _require_validsig(status, expected_primary)


def _sign(manifest, signature, home, subkey, passphrase):
    _gpg(home, '--pinentry-mode', 'loopback', '--passphrase-fd', '0',
         '--local-user', subkey + '!', '--armor', '--detach-sign',
         '--output', str(signature.resolve()), str(manifest.resolve()), input=passphrase + b'\n')


def _files(folder):
    manifest, signature = folder / 'SHA256SUMS', folder / 'SHA256SUMS.asc'
    if not manifest.is_file() or manifest.is_symlink():
        raise ValueError('SHA256SUMS must be a regular file')
    return manifest, signature


def sign(folder, verification_home):
    """Production signer: fixed subkey and independent pinned-key verification."""
    manifest, signature = _files(folder)
    home = Path(os.environ['GNUPGHOME']).resolve()
    verification_home = verification_home.resolve()
    if home == verification_home or any(home.iterdir()) or any(verification_home.iterdir()):
        raise ValueError('signing and verification need two fresh GnuPG homes')
    if signature.exists() or signature.is_symlink():
        raise ValueError('refusing to overwrite a checksum signature')
    key, passphrase = os.environ.get('RELEASE_SIGNING_KEY'), os.environ.get('RELEASE_SIGNING_PASSPHRASE')
    if not key or not passphrase:
        raise ValueError('release signing secrets are required')
    _gpg(home, '--import', input=key.encode())
    try:
        _sign(manifest, signature, home, SIGNING_SUBKEY, passphrase.encode())
        _verify(manifest, signature, verification_home, PUBLIC_KEY.read_bytes(), PRIMARY_FINGERPRINT)
    except Exception:
        signature.unlink(missing_ok=True)
        raise
    print('SHA256SUMS.asc signed and verified with the pinned release key', flush=True)


def verify(folder):
    """Production verifier: the committed key and fingerprint cannot be overridden."""
    manifest, signature = _files(folder)
    if not signature.is_file() or signature.is_symlink():
        raise ValueError('SHA256SUMS.asc must be a regular file')
    with temporary_home() as home:
        _verify(manifest, signature, home, PUBLIC_KEY.read_bytes(), PRIMARY_FINGERPRINT)
    print('OpenPGP checksum signature verified: ' + PRIMARY_FINGERPRINT, flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    signing = commands.add_parser('sign')
    signing.add_argument('folder', type=Path)
    signing.add_argument('--verification-home', required=True, type=Path)
    verifying = commands.add_parser('verify')
    verifying.add_argument('folder', type=Path)
    args = parser.parse_args()
    if args.command == 'sign':
        sign(args.folder, args.verification_home)
    else:
        verify(args.folder)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError) as error:
        sys.exit(f'error: {error}')
