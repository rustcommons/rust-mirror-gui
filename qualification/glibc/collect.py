#!/usr/bin/env python3
"""Turn one image's run-matrix output into JSON rows (CI-only measurements).

Usage: collect.py OUT_DIR IMAGE IMAGE_DIGEST BINARIES_JSON RESULT_JSON
BINARIES_JSON maps binary file name -> {"method": ..., "target_glibc": ...}.
Converts any screen dump the container could not convert (no Python 3 there)
and counts its colours, so a blank window is not reported as drawn.
"""
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent


def facts(out):
    data = {}
    path = out / "facts.txt"
    if path.exists():
        for line in path.read_text(errors="replace").splitlines():
            key, _, value = line.partition("=")
            data[key] = value
    return data


def last_lines(path, n=3):
    if not path.exists():
        return ""
    lines = [l for l in path.read_text(errors="replace").splitlines() if l.strip()]
    return " | ".join(lines[-n:])[:600]


def colours(case):
    """Distinct colours on screen: from the container's conversion (launch.log),
    or by converting the dump it left (no Python 3 in that container)."""
    import re
    xwd = case / "screen.xwd"
    if xwd.exists():
        done = subprocess.run([sys.executable, str(HERE.parent / "xwd2png.py"), str(xwd), str(case / "screen.png")],
                              capture_output=True, text=True)
        xwd.unlink()
        text = done.stdout
    else:
        text = (case / "launch.log").read_text(errors="replace") if (case / "launch.log").exists() else ""
    m = re.search(r"(\d+) distinct colours", text)
    return int(m.group(1)) if m else None


def main():
    out, image, digest, binaries, result = sys.argv[1:6]
    out = pathlib.Path(out)
    binaries = json.loads(binaries)
    f = facts(out)
    rows = []
    tsv = out / "results.tsv"
    lines = tsv.read_text().splitlines() if tsv.exists() else []
    if not lines:
        rows.append({"image": image, "image_digest": digest, "result": "not_run",
                     "failure_reason": "the container produced no results (setup failed?)", "userland": f})
    for line in lines:
        name, renderer, worker, launch, network = (line.split() + ["-"] * 5)[:5]
        case = out / f"{name}-{renderer}"
        n = colours(case)
        drawn = launch == "0" and (n is None or n >= 16) and (case / "window.txt").exists()
        if worker != "0":
            result_, reason = "failed", "worker: " + last_lines(case / "worker.txt")
        elif not drawn:
            result_, reason = "failed", "window: " + (last_lines(case / "app.log") or last_lines(case / "launch.log"))
        else:
            result_, reason = "passed", None
        app_log = last_lines(case / "app.log", 6)
        rows.append({
            "image": image, "image_digest": digest,
            "distro": f.get("distro"), "glibc": f.get("glibc"), "mesa": f.get("mesa"),
            "missing_packages": f.get("missing_packages"),
            "binary": name, **binaries.get(name, {}),
            "renderer": renderer, "worker_version": worker == "0",
            "window_drawn": drawn, "distinct_colours": n,
            "network_plan_sync": network if network != "-" else None,
            "result": result_, "failure_reason": reason, "app_log": app_log or None,
        })
    pathlib.Path(result).write_text(json.dumps(rows, indent=2) + "\n")
    for r in rows:
        print(r.get("binary"), r.get("renderer"), r["result"], r.get("network_plan_sync"), r.get("failure_reason") or "")


if __name__ == "__main__":
    main()
