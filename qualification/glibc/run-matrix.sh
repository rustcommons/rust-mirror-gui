#!/bin/sh
# Inside an old-distribution container: for every binary in /work/bin and each
# renderer (default, gl), run the worker's version check, then launch the
# window on Xvfb. Once per binary, the worker also runs a real crate plan and
# sync over HTTPS (DNS, TLS with certificate checks). Writes
# /work/out/<binary>-<renderer>/ and appends
# "binary renderer worker_exit launch_exit network" lines to /work/out/results.tsv.
set -u
mkdir -p /work/out
sh /work/qualification/glibc/setup-distro.sh /work/out/facts.txt
# Plan, then approve and sync, one small crate from crates.io (the same check
# rust-mirror's release gate uses). The worker treats end of input as Cancel,
# so input stays open while it runs.
worker_with_open_input() { # BIN STATE NAME ARGS...: stdin stays open until the worker exits
  bin=$1; state=$2; step=$3; shift 3
  rm -f "$state/in"; mkfifo "$state/in"
  sleep 600 > "$state/in" & holder=$!
  timeout 200 "$bin" --worker "$@" < "$state/in" > "$state/$step.jsonl" 2> "$state/$step.err"
  kill $holder 2>/dev/null; wait $holder 2>/dev/null
}

network_check() {
  bin=$1; out=$2; state=$out/net; mkdir -p "$state"
  cat > "$state/catalog.toml" <<'TOML'
[crates]
itoa = "=1.0.15"
[crate-policy]
hosts = ["x86_64-unknown-linux-gnu"]
targets = ["x86_64-unknown-linux-gnu"]
[crate-output]
base-url = "https://mirror.example/measurement"
[crate-network]
contact = "https://github.com/rustcommons/rust-mirror-gui/issues"
TOML
  worker_with_open_input "$bin" "$state" plan crates sync --config "$state/catalog.toml" --output "$state/run"
  plan=$(tail -n 1 "$state/plan.jsonl" | sed -n 's/.*"plan_id":"\([^"]*\)".*/\1/p')
  if ! tail -n 1 "$state/plan.jsonl" | grep -q '"status":"needs_approval"' || [ -z "$plan" ]; then echo plan_failed; return; fi
  worker_with_open_input "$bin" "$state" sync crates sync --config "$state/catalog.toml" --output "$state/run" --approve-plan "$plan"
  if tail -n 1 "$state/sync.jsonl" | grep -q '"status":"completed"'; then echo passed; else echo sync_failed; fi
}

for bin in /work/bin/*; do
  name=$(basename "$bin"); chmod +x "$bin"
  for renderer in default gl; do
    out=/work/out/$name-$renderer; mkdir -p "$out"
    "$bin" --worker --version </dev/null >"$out/worker.txt" 2>&1; worker=$?
    if [ $worker -eq 0 ]; then
      if [ $renderer = gl ]; then
        WGPU_BACKEND=gl timeout 90 sh /work/qualification/linux-launch.sh "$bin" "$out" >"$out/launch.log" 2>&1; launch=$?
      else
        timeout 90 sh /work/qualification/linux-launch.sh "$bin" "$out" >"$out/launch.log" 2>&1; launch=$?
      fi
    else
      launch=skipped
    fi
    network=-
    if [ $renderer = default ] && [ $worker -eq 0 ]; then network=$(network_check "$bin" "$out"); fi
    echo "$name $renderer $worker $launch $network" >> /work/out/results.tsv
  done
done
chmod -R a+rwX /work/out
