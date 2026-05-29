#!/usr/bin/env bash
set -u
fail=0
pass(){ echo "PASS $1"; }
fail_check(){ echo "FAIL $1"; fail=1; }

if grep -Fq 'egui_kittest = { version = "0.30"' crates/switchbard-gui/Cargo.toml; then pass MUST-001A; else fail_check MUST-001A; fi
if grep -Fq 'kittest = "0.1"' crates/switchbard-gui/Cargo.toml; then pass MUST-001B; else fail_check MUST-001B; fi
if grep -Fq 'features = ["snapshot", "wgpu"]' crates/switchbard-gui/Cargo.toml; then pass MUST-002; else fail_check MUST-002; fi
if grep -Fq 'features = ["accesskit", "default_fonts", "glow"]' crates/switchbard-gui/Cargo.toml \
  && grep -Fq 'features = ["accesskit", "default_fonts", "glow", "x11", "wayland"]' crates/switchbard-gui/Cargo.toml; then pass MUST-003; else fail_check MUST-003; fi
if test -f crates/switchbard-gui/tests/egui_kittest_harness.rs \
  && grep -Fq 'egui_kittest::Harness' crates/switchbard-gui/tests/egui_kittest_harness.rs \
  && grep -Fq 'kittest::Queryable' crates/switchbard-gui/tests/egui_kittest_harness.rs \
  && grep -Fq '.click()' crates/switchbard-gui/tests/egui_kittest_harness.rs; then pass MUST-004; else fail_check MUST-004; fi
if grep -Fq '**/tests/snapshots/**/*.diff.png' .gitignore \
  && grep -Fq '**/tests/snapshots/**/*.new.png' .gitignore; then pass MUST-005; else fail_check MUST-005; fi
exit $fail
