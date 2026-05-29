#!/usr/bin/env bash
set -u
fail=0
pass(){ echo "PASS $1"; }
fail_check(){ echo "FAIL $1"; fail=1; }

if grep -Fq 'egui_extras = { version = "0.30"' crates/switchbard-gui/Cargo.toml; then pass MUST-001; else fail_check MUST-001; fi
if grep -Fq 'eframe = { version = "0.30"' crates/switchbard-gui/Cargo.toml; then pass MUST-002; else fail_check MUST-002; fi
if grep -A1 'name = "egui"' Cargo.lock | grep -Fq 'version = "0.30.' \
  && grep -A1 'name = "eframe"' Cargo.lock | grep -Fq 'version = "0.30.' \
  && grep -A1 'name = "egui_extras"' Cargo.lock | grep -Fq 'version = "0.30.'; then
  pass MUST-003
else
  fail_check MUST-003
fi
if python3 - <<'PY'
from pathlib import Path
packages = Path('Cargo.lock').read_text().split('[[package]]')
egui_names = {'ecolor','eframe','egui','egui-wgpu','egui-winit','egui_extras','egui_glow','emath','epaint','epaint_default_fonts'}
for pkg in packages:
    name = None
    version = None
    for line in pkg.splitlines():
        if line.startswith('name = '):
            name = line.split('=', 1)[1].strip().strip('"')
        if line.startswith('version = '):
            version = line.split('=', 1)[1].strip().strip('"')
    if name in egui_names and version and version.startswith('0.29.'):
        raise SystemExit(1)
PY
then
  pass MUST-004
else
  fail_check MUST-004
fi
exit $fail
