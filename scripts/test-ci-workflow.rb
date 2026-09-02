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

sidecar = jobs.fetch("mission-sidecar")
raise "mission-sidecar must depend on change-scope" unless sidecar.fetch("needs") == "change-scope"
condition = sidecar.fetch("if")
raise "mission-sidecar condition lost" unless condition.include?("needs.change-scope.outputs.mission_sidecar")
raise "delivery preflight command lost" unless delivery.dig("commands", "lint") == "mise run preflight"

puts "CI workflow contract: PASS"
