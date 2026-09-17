#!/usr/bin/env ruby
# frozen_string_literal: true

require "yaml"

repo_root = File.expand_path("..", __dir__)
workflow = YAML.safe_load(File.read(File.join(repo_root, ".github/workflows/ci.yml")), aliases: true)
delivery = YAML.safe_load(File.read(File.join(repo_root, ".no-mistakes.yaml")), aliases: true)
triggers = workflow["on"] || workflow[true]
jobs = workflow.fetch("jobs")

raise "CI must retain pull_request" unless triggers.key?("pull_request")
raise "CI must retain push-to-main" unless triggers.dig("push", "branches") == ["main"]

matrix = jobs.dig("ci", "strategy", "matrix", "include")
expected = [
  ["ubuntu-latest", "fmt"],
  ["ubuntu-latest", "clippy"],
  ["ubuntu-latest", "test"],
  ["macos-latest", "clippy"],
  ["macos-latest", "test"]
]
actual = matrix.map { |entry| [entry.fetch("os"), entry.fetch("task")] }
raise "unexpected Rust CI matrix: #{actual.inspect}" unless actual == expected

scope = jobs.fetch("change-scope")
raise "change-scope must expose mission_sidecar" unless scope.dig("outputs", "mission_sidecar")
raise "change-scope must expose rust" unless scope.dig("outputs", "rust")
raise "change-scope must itself always run" if scope.key?("if")

# TASK-180: the Rust matrix is routed, and only by the router. A gate that
# reads anything else -- or that goes missing -- either burns the matrix on
# every prose change again or, worse, skips it on a change that needs it.
rust = jobs.fetch("ci")
raise "Rust matrix must depend on change-scope" unless rust.fetch("needs") == "change-scope"
rust_condition = rust["if"]
unless rust_condition.to_s.include?("needs.change-scope.outputs.rust")
  raise "Rust matrix routing condition lost: #{rust_condition.inspect}"
end

# Both matrices must read their route as "run unless explicitly told not to".
# `== 'true'` would turn a missing or empty output into a silent skip on a
# green run, which is the one failure mode routing must not have.
{ "ci" => "rust", "mission-sidecar" => "mission_sidecar" }.each do |job, output|
  condition = jobs.fetch(job)["if"].to_s
  unless condition.include?("needs.change-scope.outputs.#{output} != 'false'")
    raise "#{job} must fail open on a missing route: #{condition.inspect}"
  end
end

sidecar = jobs.fetch("mission-sidecar")
raise "mission-sidecar must depend on change-scope" unless sidecar.fetch("needs") == "change-scope"
condition = sidecar.fetch("if")
raise "mission-sidecar condition lost" unless condition.include?("needs.change-scope.outputs.mission_sidecar")
raise "delivery preflight command lost" unless delivery.dig("commands", "lint") == "mise run preflight"

puts "CI workflow contract: PASS"

terminal = YAML.safe_load(File.read(File.join(repo_root, ".github/workflows/release-tui.yml")), aliases: true)
terminal_triggers = terminal["on"] || terminal[true]
raise "terminal release must build PR artifacts" unless terminal_triggers.key?("pull_request")
raise "terminal release must support draft builds" unless terminal_triggers.key?("workflow_dispatch")
raise "publishing must not replace public assets" if terminal_triggers.key?("release")
raise "builds must have read-only contents permission" unless terminal.dig("permissions", "contents") == "read"
terminal_jobs = terminal.fetch("jobs")
platforms = terminal_jobs.dig("binaries", "strategy", "matrix", "include").map { |entry| entry.fetch("platform") }
raise "terminal platform coverage changed" unless platforms.sort == %w[linux-x86_64 macos-arm64 macos-x86_64]
publisher = terminal_jobs.fetch("publish")
raise "publish must wait for all platform builds" unless publisher.fetch("needs") == "binaries"
raise "PRs must not publish" unless publisher.fetch("if") == "github.event_name != 'pull_request'"
raise "only publisher needs contents write" unless publisher.dig("permissions", "contents") == "write"
puts "Terminal release workflow contract: PASS"
