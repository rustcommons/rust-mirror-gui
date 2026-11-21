#!/usr/bin/env python3
"""Release assets of rust-mirror-gui: layout, checksums, provenance, release, verification.

Standard library only (plus `git` and the GitHub CLI `gh` where noted). Used by
build-binaries-core.yml, build-binaries.yml, trusted-publishing.yml and
verify-release-download.yml; tested by scripts/test_release_assets.py.

  release_assets.py sums DIR VERSION
      DIR holds exactly the checksummed files of VERSION; write DIR/SHA256SUMS.
  release_assets.py check-build DIR VERSION
      DIR holds exactly the signed files, and SHA256SUMS equals their digests.
  release_assets.py audit-crate CRATE SOURCE
      The packaged .crate holds only regular files equal to the clean git
      commit checked out in SOURCE, with that commit as its VCS record.
  release_assets.py stage DIR VERSION OUT BUNDLE...
      Write OUT/<name>-<version>.intoto.jsonl from the Sigstore BUNDLE(s);
      its subjects must be exactly DIR's signed files.
  release_assets.py release --root ROOT --tag TAG --commit SHA --assets DIR
      Verify every asset (provenance, bundle = GitHub's stored attestation,
      SBOM bindings), then create or resume a draft release from the committed
      notes, upload missing assets with name, size and digest checks, publish,
      and check again. A published release is never modified.
  release_assets.py verify --tag TAG --output DIR [--run-target T] [--github-output F]
      Public check of a live release in the exact layout of its version.

Asset layouts (`layout()`): 0.4.3 and 0.5.0 have four archives signed by
release-binaries.yml; 0.5.1 adds the SBOM and the build-binaries.yml signer
(SLSA Build L3); 0.5.2 adds the provenance bundle; releases after 0.5.2 also
carry the .crate, listed in the one SHA256SUMS.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import urllib.parse
import urllib.request
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import provenance_bundle  # noqa: E402  (same directory)

NAME = 'rust-mirror-gui'
REPO = 'rustcommons/rust-mirror-gui'
TARGET_FILES = {
    'x86_64-pc-windows-msvc': 'x86_64-pc-windows-msvc.zip',
    'x86_64-unknown-linux-gnu': 'x86_64-unknown-linux-gnu.tar.gz',
    'aarch64-apple-darwin': 'aarch64-apple-darwin.tar.gz',
    'x86_64-apple-darwin': 'x86_64-apple-darwin.tar.gz',
}
# The only hosts the API helper talks to.
ALLOWED_HOSTS = {'api.github.com', 'uploads.github.com'}
SLSA = provenance_bundle.PROVENANCE
CYCLONEDX = 'https://cyclonedx.org/bom'
VERSION = re.compile(r'([0-9]+)\.([0-9]+)\.([0-9]+)')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def version_tuple(version):
    m = VERSION.fullmatch(version)
    require(m is not None, f'not a release version: {version}')
    return tuple(int(n) for n in m.groups())


def layout(version):
    """The exact asset set of a release of this version."""
    v = version_tuple(version)
    require(v >= (0, 4, 3), f'{version}: releases before 0.4.3 have no downloads')
    archives = sorted(f'{NAME}-{version}-{f}' for f in TARGET_FILES.values())
    sbom = f'{NAME}-{version}.cdx.json' if v >= (0, 5, 1) else None
    bundle = f'{NAME}-{version}.intoto.jsonl' if v >= (0, 5, 2) else None
    crate = f'{NAME}-{version}.crate' if v > (0, 5, 2) else None
    summed = sorted(archives + [n for n in (sbom, crate) if n])
    signed = summed + ['SHA256SUMS']
    return dict(
        version=version, archives=archives, sbom=sbom, bundle=bundle, crate=crate,
        summed=summed, signed=signed, assets=sorted(signed + ([bundle] if bundle else [])),
        # 0.4.3 and 0.5.0 keep the identity they were published with.
        signer='build-binaries.yml' if sbom else 'release-binaries.yml', strict=sbom is not None,
        sbom_subjects=sorted(archives + ([crate] if crate else [])) if sbom else [],
    )


def check_names(version, names, key='assets'):
    want, got = layout(version)[key], sorted(names)
    missing, extra = sorted(set(want) - set(got)), sorted(set(got) - set(want))
    require(not missing and not extra and len(got) == len(want),
            f'{version}: expected exactly {want}; missing {missing}, unexpected {extra}')


def sums_text(folder: Path, names):
    return ''.join(f'{digest((folder / n).read_bytes())}  {n}\n' for n in sorted(names))


def write_sums(folder: Path, version):
    check_names(version, [p.name for p in folder.iterdir()], 'summed')
    text = sums_text(folder, layout(version)['summed'])
    (folder / 'SHA256SUMS').write_bytes(text.encode())
    return text


def check_build(folder: Path, version):
    lay = layout(version)
    check_names(version, [p.name for p in folder.iterdir()], 'signed')
    require((folder / 'SHA256SUMS').read_bytes().decode() == sums_text(folder, lay['summed']),
            'SHA256SUMS differs from the files')
    return lay


def audit_crate(crate: Path, source: Path):
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip()
    require(subprocess.check_output(['git', 'status', '--porcelain'], cwd=source, text=True) == '',
            'the source checkout is not clean')
    prefix = crate.name.removesuffix('.crate') + '/'
    with tarfile.open(crate) as archive:
        members = archive.getmembers()
        require(bool(members) and all(m.isfile() and m.name.startswith(prefix) for m in members),
                'only regular files under one root')
        names = [m.name[len(prefix):] for m in members]
        require(not any(n.startswith(('fuzz/', '.clusterfuzzlite/', '.github/', 'docs/', 'qualification/', 'scripts/'))
                        for n in names), 'fuzz/CI/docs content packaged')
        for m, name in zip(members, names):
            if name in ('Cargo.toml', '.cargo_vcs_info.json'):
                continue
            path = 'Cargo.toml' if name == 'Cargo.toml.orig' else name
            committed = subprocess.check_output(['git', 'show', f'HEAD:{path}'], cwd=source)
            require(archive.extractfile(m).read() == committed, f'{name} differs from the commit')
        vcs = json.loads(archive.extractfile(prefix + '.cargo_vcs_info.json').read())
    require(vcs['git']['sha1'] == head and not vcs['git'].get('dirty', False), 'VCS record is not this clean commit')
    return len(members), head


def stage(folder: Path, version, out: Path, bundles):
    """Write the release's provenance bundle; its subjects must be exactly the signed files."""
    lay = check_build(folder, version)
    out.mkdir(parents=True, exist_ok=True)
    target = out / lay['bundle']
    require(not target.exists(), 'provenance bundle already staged')
    provenance_bundle.assemble(str(target), [str(b) for b in bundles])
    provenance_bundle.check(str(target), [str(folder / n) for n in lay['signed']])
    return target


# --- GitHub API -------------------------------------------------------------

def api(url, method='GET', data=None, content_type='application/json'):
    """Call the GitHub API: only HTTPS to ALLOWED_HOSTS, no proxy, no redirects."""
    parts = urllib.parse.urlsplit(url)
    require(parts.scheme == 'https' and parts.hostname in ALLOWED_HOSTS and parts.port is None
            and not parts.username and not parts.password, f'refusing a non-GitHub API URL: {url}')
    headers = {'Accept': 'application/vnd.github+json', 'User-Agent': 'rust-mirror-gui-release'}
    token = os.environ.get('GH_TOKEN') or os.environ.get('GITHUB_TOKEN')
    if token:
        headers['Authorization'] = f'Bearer {token}'
    if data is not None:
        headers['Content-Type'] = content_type
    request = urllib.request.Request(url, headers=headers, data=data, method=method)
    no_redirect = type('NoRedirect', (urllib.request.HTTPRedirectHandler,), {'redirect_request': lambda *a, **k: None})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), no_redirect())
    with opener.open(request, timeout=120) as response:
        return json.load(response)


def notes_title(notes):
    title = next((line[2:].strip() for line in notes.splitlines() if line.startswith('# ')), '')
    require(bool(title), 'release notes have no first-level heading')
    return title


def same_body(a, b):
    return (a or '').replace('\r\n', '\n').strip() == (b or '').replace('\r\n', '\n').strip()


def check_release_state(release, tag, notes, draft):
    """Refuse a release that is not exactly ours: tag, title, body, draft and prerelease state."""
    require(release['tag_name'] == tag, 'release is on a different tag')
    require(release['name'] == notes_title(notes), 'release title differs from the committed notes')
    require(same_body(release.get('body'), notes), 'release body differs from the committed notes')
    require(release['draft'] is draft and release['prerelease'] is False, 'release draft/prerelease state is not as expected')


def publish_release(tag, notes, folder: Path, call=None):
    """Create or resume the draft release, upload only missing assets, check them, then publish.

    A killed run leaves a draft (the tags API returns 404 for drafts), so drafts are
    found through the releases list. Existing assets must equal the local bytes; none
    is ever replaced or deleted. A published release must already be complete and is
    not modified.
    """
    call = call or api
    base = f'https://api.github.com/repos/{REPO}'
    files = {p.name: p for p in sorted(folder.iterdir())}
    want = {n: 'sha256:' + digest(p.read_bytes()) for n, p in files.items()}
    sizes = {n: p.stat().st_size for n, p in files.items()}
    matches, page = [], 1
    while True:
        batch = call(f'{base}/releases?per_page=100&page={page}')
        matches += [r for r in batch if r['tag_name'] == tag]
        if len(batch) < 100:
            break
        page += 1
    require(len(matches) <= 1, 'several releases exist for the tag')
    if matches:
        release = matches[0]
    else:
        release = call(f'{base}/releases', 'POST', json.dumps(
            {'tag_name': tag, 'name': notes_title(notes), 'body': notes, 'draft': True, 'prerelease': False}).encode())
        print('Created draft release', release['id'], flush=True)
    check_release_state(release, tag, notes, release['draft'])
    existing = {a['name']: a for a in release['assets']}
    require(set(existing) <= set(want), f'release holds an unexpected asset: {sorted(set(existing) - set(want))}')
    for name, asset in existing.items():
        require(asset.get('digest') == want[name] and asset['size'] == sizes[name] and asset['state'] == 'uploaded',
                f'existing asset {name} differs or is incomplete; never replacing it (see RELEASING.md, manual recovery)')
    if not release['draft']:
        require(set(existing) == set(want), 'published release is incomplete; refusing to modify a published release')
        print('Release already published with every asset', flush=True)
        return release
    for name in sorted(set(want) - set(existing)):
        raw = files[name].read_bytes()
        query = urllib.parse.urlencode({'name': name})
        result = call(f'https://uploads.github.com/repos/{REPO}/releases/{release["id"]}/assets?{query}', 'POST', raw,
                      'application/octet-stream')
        require(result['name'] == name and result['size'] == len(raw) and result.get('digest') == want[name],
                f'uploaded asset {name} mismatch')
        print('Uploaded:', name, want[name], flush=True)
    release = call(f'{base}/releases/{release["id"]}')
    check_release_state(release, tag, notes, True)
    require({a['name']: (a.get('digest'), a['size']) for a in release['assets']} == {n: (want[n], sizes[n]) for n in want}
            and all(a['state'] == 'uploaded' for a in release['assets']), 'draft assets incomplete before publishing')
    call(f'{base}/releases/{release["id"]}', 'PATCH', json.dumps({'draft': False, 'make_latest': 'true'}).encode())
    release = call(f'{base}/releases/tags/{tag}')
    check_release_state(release, tag, notes, False)
    require({a['name']: a.get('digest') for a in release['assets']} == want, 'published assets differ')
    print('Published', tag, 'with', len(want), 'assets', flush=True)
    return release


# --- attestations (gh) --------------------------------------------------------

def policy(lay, commit):
    flags = ['--repo', REPO, '--signer-workflow', f'{REPO}/.github/workflows/{lay["signer"]}']
    if lay['strict']:
        require(re.fullmatch(r'[0-9a-f]{40}', commit or '') is not None, 'strict verification needs the exact commit')
        flags += ['--signer-digest', commit, '--source-digest', commit, '--deny-self-hosted-runners']
    return flags


def verify_attestations(folder: Path, lay, commit, bundle: Path | None):
    """Provenance of every signed file (online and with the bundle), the bundle against
    GitHub's store, and the SBOM attestation of each archive (and the crate)."""
    flags = policy(lay, commit)
    signed = [folder / n for n in lay['signed']]
    for path in signed:
        subprocess.run(['gh', 'attestation', 'verify', str(path), *flags], check=True, stdout=subprocess.DEVNULL)
        print(f'provenance verified ({lay["signer"]}): {path.name}', flush=True)
    store = Path(tempfile.mkdtemp(prefix='gui-attestations-'))
    for path in signed:
        subprocess.run(['gh', 'attestation', 'download', str(path.resolve()), '--repo', REPO, '--predicate-type', SLSA],
                       check=True, cwd=store, stdout=subprocess.DEVNULL)
    stored = sorted(store.glob('*.jsonl'))  # sha256:<d>.jsonl, or sha256-<d>.jsonl on Windows
    require(bool(stored), 'no stored attestations were downloaded')
    if bundle is None:  # releases before 0.5.2: check one built from the store
        bundle = store.parent / f'{store.name}-assembled.intoto.jsonl'
        provenance_bundle.assemble(str(bundle), [str(p) for p in stored])
    provenance_bundle.check(str(bundle), [str(p) for p in signed])
    provenance_bundle.match(str(bundle), [str(p) for p in stored])
    for path in signed:
        subprocess.run(['gh', 'attestation', 'verify', str(path), '--bundle', str(bundle), *flags],
                       check=True, stdout=subprocess.DEVNULL)
        print(f'verified offline with {bundle.name}: {path.name}', flush=True)
    if lay['sbom']:
        sbom = json.loads((folder / lay['sbom']).read_bytes().decode('utf-8'))
        for name in lay['sbom_subjects']:
            proof = subprocess.check_output(['gh', 'attestation', 'verify', str(folder / name), *flags,
                                             '--predicate-type', CYCLONEDX, '--format', 'json'])
            results = json.loads(proof.decode('utf-8'))
            require(any(r['verificationResult']['statement']['predicate'] == sbom for r in results),
                    f'the attested SBOM of {name} differs from {lay["sbom"]}')
            print(f'SBOM attestation verified, same SBOM: {name}', flush=True)


def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root, text=True).strip()


def release_command(args):
    tag, commit, folder = args.tag, args.commit, args.assets
    require(re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+', tag) is not None, 'not a release tag')
    require(git(args.root, 'rev-parse', f'refs/tags/{tag}^{{commit}}') == commit, 'the tag is not this commit')
    live = dict(line.split('\t')[::-1] for line in git(args.root, 'ls-remote', 'origin', f'refs/tags/{tag}',
                                                       f'refs/tags/{tag}^{{}}').splitlines())
    require(live.get(f'refs/tags/{tag}^{{}}', live.get(f'refs/tags/{tag}')) == commit, 'the live tag moved')
    version = tag[1:]
    lay = layout(version)
    check_names(version, [p.name for p in folder.iterdir()])
    stash = Path(tempfile.mkdtemp(prefix='gui-signed-'))
    for n in lay['signed']:
        (stash / n).write_bytes((folder / n).read_bytes())
    check_build(stash, version)
    verify_attestations(folder, lay, commit, folder / lay['bundle'])
    notes = subprocess.check_output(['git', 'show', f'{commit}:docs/releases/{tag}.md'], cwd=args.root).decode('utf-8')
    publish_release(tag, notes, folder)


def smoke(archive: Path, target, out: Path):
    """Unpack the downloaded archive and run its built-in worker."""
    out.mkdir(parents=True, exist_ok=True)
    if archive.suffix == '.zip':
        with zipfile.ZipFile(archive) as z:
            z.extractall(out)
    else:
        with tarfile.open(archive) as t:
            t.extractall(out, filter='data')
    exe = next(p for p in out.rglob(NAME + ('.exe' if target.endswith('msvc') else '')) if p.is_file())
    result = subprocess.run([str(exe), '--worker', '--version'], stdin=subprocess.DEVNULL, capture_output=True, check=True)
    hello = json.loads(result.stdout.decode('utf-8').splitlines()[0])
    require(hello.get('type') == 'hello' and hello.get('protocol') == 1, f'unexpected worker hello: {hello}')
    print(f'{exe.name} --worker --version: protocol 1, engine {hello.get("tool_version")}', flush=True)


def verify_command(args, call=None):
    call = call or api
    tag = args.tag
    version = tag[1:]
    lay = layout(version)
    release = call(f'https://api.github.com/repos/{REPO}/releases/tags/{tag}')
    require(release['tag_name'] == tag and not release['draft'] and not release['prerelease'], 'not a published release')
    assets = {a['name']: a for a in release['assets']}
    check_names(version, assets)
    require(all(a['state'] == 'uploaded' for a in assets.values()), 'an asset is incomplete')
    out = args.output
    out.mkdir(parents=True, exist_ok=False)
    subprocess.run(['gh', 'release', 'download', tag, '--repo', REPO, '--dir', str(out)], check=True)
    check_names(version, [p.name for p in out.iterdir()])
    for name, asset in assets.items():
        actual = digest((out / name).read_bytes())
        if asset.get('digest'):
            require(asset['digest'] == 'sha256:' + actual, f'{name}: GitHub asset digest mismatch')
        print(f'{actual}  {name}', flush=True)
    require((out / 'SHA256SUMS').read_bytes().decode() == sums_text(out, lay['summed']), 'SHA256SUMS differs from the files')
    commit = call(f'https://api.github.com/repos/{REPO}/commits/{tag}')['sha'] if lay['strict'] else None
    verify_attestations(out, lay, commit, out / lay['bundle'] if lay['bundle'] else None)
    if args.run_target:
        smoke(out / f'{NAME}-{version}-{TARGET_FILES[args.run_target]}', args.run_target, out.parent / 'run')
    if args.github_output:
        sums = {line.split('  ')[1]: line.split('  ')[0] for line in (out / 'SHA256SUMS').read_text().splitlines()}
        with open(args.github_output, 'a', encoding='utf-8') as f:
            for key, target in (('windows', 'x86_64-pc-windows-msvc'), ('linux', 'x86_64-unknown-linux-gnu')):
                f.write(key + '=' + sums[NAME + '-' + version + '-' + TARGET_FILES[target]] + '\n')
    print(f'{tag}: all {len(assets)} assets verified in the {version} layout', flush=True)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest='command', required=True)
    s = sub.add_parser('sums'); s.add_argument('dir', type=Path); s.add_argument('version')
    s = sub.add_parser('check-build'); s.add_argument('dir', type=Path); s.add_argument('version')
    s = sub.add_parser('audit-crate'); s.add_argument('crate', type=Path); s.add_argument('source', type=Path)
    s = sub.add_parser('stage'); s.add_argument('dir', type=Path); s.add_argument('version'); s.add_argument('out', type=Path)
    s.add_argument('bundles', type=Path, nargs='+')
    s = sub.add_parser('release'); s.add_argument('--root', type=Path, required=True); s.add_argument('--tag', required=True)
    s.add_argument('--commit', required=True); s.add_argument('--assets', type=Path, required=True)
    s = sub.add_parser('verify'); s.add_argument('--tag', required=True); s.add_argument('--output', type=Path, required=True)
    s.add_argument('--run-target', choices=sorted(TARGET_FILES)); s.add_argument('--github-output')
    args = p.parse_args(argv)
    if args.command == 'sums':
        print(write_sums(args.dir, args.version), end='')
    elif args.command == 'check-build':
        check_build(args.dir, args.version); print(f'{args.dir}: exactly the {args.version} build files; SHA256SUMS matches')
    elif args.command == 'audit-crate':
        n, head = audit_crate(args.crate, args.source); print(f'Package audit passed: {n} regular files, VCS {head}.')
    elif args.command == 'stage':
        print('staged', stage(args.dir, args.version, args.out, args.bundles))
    elif args.command == 'release':
        release_command(args)
    else:
        verify_command(args)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(f'error: {error}')
