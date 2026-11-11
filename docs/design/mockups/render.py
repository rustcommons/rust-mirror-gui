#!/usr/bin/env python3
"""Render the static design mockups to PNG with headless Chromium.

Usage: python3 docs/design/mockups/render.py [CHROMIUM]

Each `<screen>.html` file holds only the main panel. This script wraps it in the
shared window chrome, writes the page to a temporary directory and screenshots it
at 1440x900 into `docs/design/<screen>.png`. Mockups only; this is not app code.
"""
import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE.parent
CHROMIUM = sys.argv[1] if len(sys.argv) > 1 else "chromium"

# (file stem, nav index, page title)
SCREENS = [
    ("selection-builder", 0, "Selections"),
    ("plan-preview", 1, "Plan preview"),
    ("sync-progress", 2, "Sync"),
    ("publish-nexus", 4, "Publish"),
    ("retention", 3, "Generations"),
]
NAV = ["Selections", "Plan preview", "Sync &amp; finish", "Generations", "Publish"]

SHELL = """<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="{css}"><title>{title}</title></head><body>
<div class="app">
  <div class="top">
    <div class="brand">rust-mirror <span>GUI</span></div>
    <div class="ws">Workspace <b>D:\\mirrors\\rust</b> &nbsp;·&nbsp; Config <b>mirror-002.toml</b></div>
    <div class="spacer"></div>
    <div class="ws">Rustup <b>1.28.2</b> &nbsp;·&nbsp; Budget <b>10 GiB</b></div>
    <span class="btn">Open workspace…</span>
  </div>
  <div class="nav">
    <h4>Prepare (connected machine)</h4>
    {nav_prepare}
    <h4>Manage</h4>
    {nav_manage}
    <h4>Deliver (offline network)</h4>
    {nav_deliver}
    <h4>Help</h4>
    <a class="muted">User guide</a><a class="muted">Command equivalents</a>
  </div>
  <div class="main">{body}</div>
  <div class="status">
    <span>rust-mirror library 0.1.1</span><span>Upstream: static.rust-lang.org (HTTPS)</span>
    <span style="flex:1"></span><span>Mockup — not implemented</span>
  </div>
</div></body></html>"""


def nav_item(i, current):
    cls = "on" if i == current else ("done" if i < current and current != 3 else "")
    return f'<a class="{cls}"><span class="n">{i + 1}</span>{NAV[i]}</a>'


def main():
    css = (HERE / "style.css").as_uri()
    with tempfile.TemporaryDirectory() as tmp:
        for stem, current, title in SCREENS:
            body = (HERE / f"{stem}.html").read_text(encoding="utf-8")
            page = SHELL.format(
                css=css,
                title=title,
                body=body,
                nav_prepare="".join(nav_item(i, current) for i in (0, 1, 2)),
                nav_manage=nav_item(3, current),
                nav_deliver=nav_item(4, current),
            )
            html = pathlib.Path(tmp) / f"{stem}.html"
            html.write_text(page, encoding="utf-8")
            png = OUT / f"{stem}.png"
            subprocess.run(
                [
                    CHROMIUM, "--headless", "--no-sandbox", "--disable-gpu",
                    "--hide-scrollbars", f"--user-data-dir={tmp}/profile",
                    "--window-size=1440,900", f"--screenshot={png}", html.as_uri(),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            print(png)


if __name__ == "__main__":
    main()
