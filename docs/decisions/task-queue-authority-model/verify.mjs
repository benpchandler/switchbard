#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { homedir } from "node:os";

const decisionDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(decisionDir, "../../..");
const tmpDir = resolve(repoRoot, "tmp");
const summaryPath = resolve(tmpDir, "task-queue-authority-model-acceptance.json");
const liveEvidencePath = resolve(tmpDir, "task-queue-authority-model-live-lucella.json");
const visualEvidencePath = resolve(tmpDir, "task-queue-authority-model-visual-review.json");
const mutationEvidencePath = resolve(tmpDir, "task-queue-authority-model-mutations.json");
const pytestXmlPath = resolve(tmpDir, "task-queue-authority-model-pytest.xml");
const shellCanonicalPath = resolve(homedir(), ".lavish/switchbard-ia-places.html");
const bodyCanonicalRelativePath =
  "docs/decisions/task-queue-authority-model/task-queue-visual-canonical.html";
const bodyCanonicalPath = repoPath(bodyCanonicalRelativePath);

mkdirSync(tmpDir, { recursive: true });

function git(...args) {
  const result = spawnSync("git", ["-C", repoRoot, ...args], {
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  });
  return result.status === 0 ? result.stdout.trim() : "unknown";
}

const commit = git("rev-parse", "HEAD");
const timestamp = new Date().toISOString();
const runnerMemo = new Map();

// The xplan verifier linter currently recognizes shell-style registrations.
// This machine-readable mirror lets that deterministic pre-gate validate the
// JavaScript verifier's exact id/kind/check-type table; executable criterion
// construction and evidence evaluation remain below.
const verifierLintRegistrations = String.raw`
record MUST-001 contract named-test
record MUST-002 behavior named-test
record MUST-003 behavior named-test
record MUST-004 behavior named-test
record MUST-005 behavior named-test
record MUST-006 contract named-test
record MUST-007 behavior named-test
record MUST-008 behavior named-test
record MUST-009 behavior named-test
record MUST-010 behavior named-test
record MUST-011 quality measurement
record MUST-012 behavior named-test
record MUST-013 quality measurement
record MUST-014 behavior api-roundtrip
record MUST-015 behavior named-test
record MUST-016 visual visual-review
record MUST-017 behavior named-test
`;
void verifierLintRegistrations;

function repoPath(path) {
  return resolve(repoRoot, path);
}

function displayPath(path) {
  return relative(repoRoot, path) || ".";
}

function shellCommand(command, args) {
  return [command, ...args]
    .map((part) => (/^[A-Za-z0-9_./:=+-]+$/.test(part) ? part : JSON.stringify(part)))
    .join(" ");
}

function runOnce({ key, command, args, cwd = repoRoot, requiredPaths = [] }) {
  if (runnerMemo.has(key)) {
    return runnerMemo.get(key);
  }
  const missing = requiredPaths
    .map(repoPath)
    .filter((path) => !existsSync(path))
    .map(displayPath);
  if (missing.length > 0) {
    const result = {
      key,
      command: shellCommand(command, args),
      cwd: displayPath(cwd),
      status: null,
      stdout: "",
      stderr: "",
      missing,
      executed: false,
    };
    runnerMemo.set(key, result);
    return result;
  }
  const processResult = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    env: process.env,
    maxBuffer: 64 * 1024 * 1024,
  });
  const result = {
    key,
    command: shellCommand(command, args),
    cwd: displayPath(cwd),
    status: processResult.status,
    signal: processResult.signal,
    error: processResult.error?.message ?? null,
    stdout: processResult.stdout ?? "",
    stderr: processResult.stderr ?? "",
    missing: [],
    executed: true,
  };
  runnerMemo.set(key, result);
  return result;
}

const coreRunner = () =>
  runOnce({
    key: "core-contract-tests",
    command: "cargo",
    args: [
      "test",
      "-p",
      "switchbard-core",
      "--test",
      "task_queue_authority_contract",
      "--",
      "--nocapture",
    ],
    requiredPaths: ["crates/switchbard-core/tests/task_queue_authority_contract.rs"],
  });

const adoptionReservationRunner = () =>
  runOnce({
    key: "adoption-reservation-focused-test",
    command: "cargo",
    args: [
      "test",
      "-p",
      "switchbard-core",
      "must_008_adoption_reservation_wrapper_is_sibling_only",
      "--",
      "--nocapture",
    ],
    requiredPaths: ["crates/switchbard-core/src/backlog/adoption.rs"],
  });

const taskRunner = () =>
  runOnce({
    key: "task-cli-contract-tests",
    command: "cargo",
    args: [
      "test",
      "-p",
      "switchbard-task",
      "--test",
      "task_queue_authority_contract",
      "--",
      "--nocapture",
    ],
    requiredPaths: ["crates/switchbard-task/tests/task_queue_authority_contract.rs"],
  });

const guiRunner = () =>
  runOnce({
    key: "gui-state-tests",
    command: "cargo",
    args: [
      "test",
      "-p",
      "switchbard-gui",
      "--test",
      "task_queue_github_states",
      "--",
      "--nocapture",
    ],
    requiredPaths: ["crates/switchbard-gui/tests/task_queue_github_states.rs"],
  });

const perfRunner = () =>
  runOnce({
    key: "gui-performance-test",
    command: "cargo",
    args: [
      "test",
      "-p",
      "switchbard-gui",
      "--test",
      "task_queue_github_perf_smoke",
      "--",
      "--nocapture",
    ],
    requiredPaths: ["crates/switchbard-gui/tests/task_queue_github_perf_smoke.rs"],
  });

const orchestratorRunner = () =>
  runOnce({
    key: "orchestrator-contract-tests",
    command: "uv",
    args: [
      "run",
      "pytest",
      "-q",
      "tests/test_task_queue_authority_contract.py",
      `--junitxml=${pytestXmlPath}`,
    ],
    cwd: repoPath("orchestrator"),
    requiredPaths: ["orchestrator/tests/test_task_queue_authority_contract.py"],
  });

const liveRunner = () =>
  runOnce({
    key: "live-lucella-probe",
    command: "node",
    args: [
      "scripts/probe_task_queue_lucella.mjs",
      "--repo-root",
      repoRoot,
      "--project-number",
      "3",
      "--read-only",
      "--out",
      liveEvidencePath,
    ],
    requiredPaths: ["scripts/probe_task_queue_lucella.mjs"],
  });

function runnerText(runner) {
  return `${runner.stdout}\n${runner.stderr}`;
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function rustNamedTest(runner, name) {
  if (!runner.executed) {
    return {
      passed: false,
      evidence: `${runner.key} not executed; missing ${runner.missing.join(", ")}`,
    };
  }
  const pattern = new RegExp(
    `^test (?:[^\\s]+::)*${escapeRegExp(name)} \\.\\.\\. (ok|FAILED|ignored)$`,
    "m",
  );
  const match = runnerText(runner).match(pattern);
  if (!match) {
    const suffix = runner.status === 0 ? "zero matching named tests" : `runner exit ${runner.status}`;
    return {
      passed: false,
      evidence: `${runner.key} did not report ${name}; ${suffix}`,
    };
  }
  return {
    passed: match[1] === "ok",
    evidence: `${runner.key} reported ${name} ... ${match[1]}`,
  };
}

function pytestNamedTest(runner, name) {
  if (!runner.executed) {
    return {
      passed: false,
      evidence: `${runner.key} not executed; missing ${runner.missing.join(", ")}`,
    };
  }
  if (!existsSync(pytestXmlPath)) {
    return {
      passed: false,
      evidence: `${runner.key} produced no ${displayPath(pytestXmlPath)}; runner exit ${runner.status}`,
    };
  }
  const xml = readFileSync(pytestXmlPath, "utf8");
  const escapedName = escapeRegExp(name);
  const paired = new RegExp(
    `<testcase\\b(?=[^>]*\\bname=["']${escapedName}["'])[^>]*>([\\s\\S]*?)<\\/testcase>`,
  ).exec(xml);
  const selfClosing = new RegExp(
    `<testcase\\b(?=[^>]*\\bname=["']${escapedName}["'])[^>]*/>`,
  ).test(xml);
  if (!paired && !selfClosing) {
    return {
      passed: false,
      evidence: `${runner.key} JUnit did not report ${name}; runner exit ${runner.status}`,
    };
  }
  const body = paired?.[1] ?? "";
  const failed = /<(?:failure|error|skipped)\b/.test(body);
  return {
    passed: !failed,
    evidence: `${runner.key} JUnit reported ${name} ${failed ? "failed/skipped" : "passed"}`,
  };
}

function metricFromRunner(runner, id) {
  if (!runner.executed) {
    return { metric: null, error: `${runner.key} was not executed` };
  }
  const prefix = `TASK_QUEUE_AUTHORITY_METRIC ${id} `;
  const line = runnerText(runner)
    .split(/\r?\n/)
    .find((candidate) => candidate.startsWith(prefix));
  if (!line) {
    return { metric: null, error: `${runner.key} emitted no ${id} metric summary` };
  }
  try {
    return { metric: JSON.parse(line.slice(prefix.length)), error: null };
  } catch (error) {
    return { metric: null, error: `${runner.key} emitted invalid ${id} JSON: ${error.message}` };
  }
}

function resultObject(id, kind, label, checkType, status, evidence, metric = null) {
  return {
    id,
    kind,
    label,
    status,
    check_type: checkType,
    evidence,
    metric,
  };
}

function namedCriterion({ id, kind, label, checks }) {
  const failed = checks.filter((check) => !check.passed);
  return resultObject(
    id,
    kind,
    label,
    "named-test",
    failed.length === 0 ? "pass" : "fail",
    checks.map((check) => check.evidence).join("; "),
  );
}

function readJsonEvidence(path) {
  if (!existsSync(path)) {
    return { value: null, error: `${displayPath(path)} is missing` };
  }
  try {
    return { value: JSON.parse(readFileSync(path, "utf8")), error: null };
  } catch (error) {
    return { value: null, error: `${displayPath(path)} is invalid JSON: ${error.message}` };
  }
}

function sha256File(path) {
  if (!existsSync(path)) {
    return null;
  }
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function bodyCanonicalMatchesRevision(revision, expectedSha256) {
  if (typeof revision !== "string" || !/^[0-9a-f]{40}$/.test(revision)) {
    return false;
  }
  const result = spawnSync(
    "git",
    ["-C", repoRoot, "show", `${revision}:${bodyCanonicalRelativePath}`],
    { maxBuffer: 16 * 1024 * 1024 },
  );
  if (result.status !== 0 || !Buffer.isBuffer(result.stdout)) {
    return false;
  }
  return createHash("sha256").update(result.stdout).digest("hex") === expectedSha256;
}

const criteria = [];

criteria.push(
  namedCriterion({
    id: "MUST-001",
    kind: "contract",
    label: "Distinct local, binding, membership, GitHub node, link, and projection identities round-trip without coordinate-derived joins.",
    checks: [rustNamedTest(coreRunner(), "must_001_distinct_identities_and_memberships")],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-002",
    kind: "behavior",
    label: "GitHub refresh has zero GitHub or native-task mutation capability and preserves task bytes on every outcome.",
    checks: [rustNamedTest(coreRunner(), "must_002_refresh_has_no_mutation_capability")],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-003",
    kind: "behavior",
    label: "Ordered github.com Project bindings round-trip atomically and invalid source mutations have zero effects.",
    checks: [rustNamedTest(coreRunner(), "must_003_bindings_round_trip_atomically_in_order")],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-004",
    kind: "behavior",
    label: "Core, Rust fallback, sb queue, and GUI consume one ranked local queue order without an id re-sort.",
    checks: [
      rustNamedTest(coreRunner(), "must_004_core_selector_preserves_ranked_order"),
      rustNamedTest(taskRunner(), "must_004_sb_queue_preserves_shared_order"),
      rustNamedTest(guiRunner(), "must_004_gui_uses_shared_local_order"),
    ],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-005",
    kind: "behavior",
    label: "Source bands, membership order, history exclusion, duplicate provenance, and linked-row suppression are deterministic.",
    checks: [rustNamedTest(coreRunner(), "must_005_source_bands_are_deterministic")],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-006",
    kind: "contract",
    label: "Observation algebra preserves Unknown, MissingOrInaccessible, Fresh, Stale, Unavailable, partial, and rate-limited states.",
    checks: [rustNamedTest(coreRunner(), "must_006_observation_states_preserve_unknowns")],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-007",
    kind: "behavior",
    label: "Newest-generation publication and the bounded viewer-bound cache are atomic, isolated, and lossless for native work.",
    checks: [rustNamedTest(coreRunner(), "must_007_generation_and_cache_are_bounded_and_isolated")],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-008",
    kind: "behavior",
    label: "Generic URL links and concurrent adoption use the sibling-only reservation wrapper without broadening ordinary ID reservation.",
    checks: [
      rustNamedTest(coreRunner(), "must_008_links_and_adoption_are_race_safe"),
      rustNamedTest(
        adoptionReservationRunner(),
        "must_008_adoption_reservation_wrapper_is_sibling_only",
      ),
    ],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-009",
    kind: "behavior",
    label: "Atomic dispatch success records label, status, composite optional note, and PR reference once across every consumer.",
    checks: [
      rustNamedTest(coreRunner(), "must_009_atomic_dispatch_success"),
      rustNamedTest(taskRunner(), "must_009_queue_release_uses_atomic_success"),
      pytestNamedTest(orchestratorRunner(), "test_must_009_release_uses_atomic_cli_boundary"),
    ],
  }),
);

criteria.push(
  namedCriterion({
    id: "MUST-010",
    kind: "behavior",
    label: "Remote delivery observations never mark native outcome, acceptance, rank, or custody complete.",
    checks: [
      rustNamedTest(coreRunner(), "must_010_remote_evidence_never_completes_local"),
      rustNamedTest(guiRunner(), "must_010_gui_remote_evidence_never_marks_done"),
    ],
  }),
);

{
  const named = rustNamedTest(coreRunner(), "must_011_adapter_budget_and_failure_taxonomy");
  const measured = metricFromRunner(coreRunner(), "MUST-011");
  const metric = measured.metric;
  const metricPassed =
    metric !== null &&
    metric.projectPageRequests === 5 &&
    metric.projectPageSize === 100 &&
    metric.retainedMemberships === 500 &&
    metric.enrichedArtifacts === 20 &&
    metric.detailRequestBudgetPerArtifact === 3 &&
    Number.isFinite(metric.observedMaxDetailRequestsPerArtifact) &&
    metric.observedMaxDetailRequestsPerArtifact <= 3 &&
    metric.totalRequests === 65 &&
    metric.request66Attempted === false &&
    metric.closingPullRequestCap === 10 &&
    Number.isFinite(metric.observedMaxClosingPullRequests) &&
    metric.observedMaxClosingPullRequests <= 10 &&
    metric.reviewCap === 50 &&
    Number.isFinite(metric.observedMaxReviews) &&
    metric.observedMaxReviews <= 50 &&
    metric.commitCap === 100 &&
    Number.isFinite(metric.observedMaxCommits) &&
    metric.observedMaxCommits <= 100 &&
    metric.checkRunCap === 100 &&
    Number.isFinite(metric.observedMaxCheckRuns) &&
    metric.observedMaxCheckRuns <= 100 &&
    metric.releaseCap === 20 &&
    Number.isFinite(metric.observedMaxReleases) &&
    metric.observedMaxReleases <= 20 &&
    metric.deploymentCap === 20 &&
    Number.isFinite(metric.observedMaxDeployments) &&
    metric.observedMaxDeployments <= 20 &&
    metric.descendantOverflowFixtures === 6 &&
    metric.descendantHasNextPageFixtures === 6 &&
    metric.descendantFollowupPageRequests === 0 &&
    metric.descendantConnectionsOverCap === 0 &&
    metric.linkedFirstThenProjectOrder === true &&
    metric.newFormatIdsOpaque === true &&
    metric.typenameObserved === true &&
    metric.failureTaxonomyComplete === true;
  criteria.push(
    resultObject(
      "MUST-011",
      "quality",
      "The adapter enforces the exact request, membership, enrichment, detail, descendant-result, and no-descendant-pagination caps with honest failure taxonomy.",
      "measurement",
      named.passed && metricPassed ? "pass" : "fail",
      [named.evidence, measured.error ?? (metricPassed ? "measured budget matches every threshold" : "measured budget does not match the contract")].join("; "),
      metric,
    ),
  );
}

criteria.push(
  namedCriterion({
    id: "MUST-012",
    kind: "behavior",
    label: "The live Tasks / Dispatches state-and-stress matrix is honest and conserves the separate dispatch history controls and order.",
    checks: [rustNamedTest(guiRunner(), "must_012_state_stress_and_history_conservation")],
  }),
);

{
  const named = rustNamedTest(perfRunner(), "must_013_500_item_task_queue_perf_budget");
  const measured = metricFromRunner(perfRunner(), "MUST-013");
  const metric = measured.metric;
  const metricPassed =
    metric !== null &&
    metric.observedItems === 500 &&
    metric.disclosedRows === 100 &&
    metric.frames === 200 &&
    Number.isFinite(metric.frameP95Ms) &&
    metric.frameP95Ms < 40;
  criteria.push(
    resultObject(
      "MUST-013",
      "quality",
      "Exactly 500 observed items, 100 disclosed rows, and 200 rendered frames remain below 40 ms frame p95.",
      "measurement",
      named.passed && metricPassed ? "pass" : "fail",
      [named.evidence, measured.error ?? (metricPassed ? "measured performance matches every threshold" : "measured performance does not match the contract")].join("; "),
      metric,
    ),
  );
}

{
  const runner = liveRunner();
  const evidenceRead = readJsonEvidence(liveEvidencePath);
  const live = evidenceRead.value;
  const allowedBlocks = new Set([
    "CredentialsUnavailable",
    "ScopeUnavailable",
    "ProjectUnavailable",
    "GitHubUnavailable",
  ]);
  let status = "fail";
  let evidence = runner.executed
    ? `live probe exit ${runner.status}`
    : `live probe not executed; missing ${runner.missing.join(", ")}`;
  let metric = null;
  if (runner.executed && live?.status === "blocked" && allowedBlocks.has(live.externalBlockCode)) {
    status = "blocked";
    evidence = `executed live probe reported allowed external block ${live.externalBlockCode}: ${live.reason ?? "no reason supplied"}`;
  } else if (runner.executed && live?.status === "pass") {
    const orderedItems = Array.isArray(live.orderedItems) ? live.orderedItems : [];
    const passed =
      runner.status === 0 &&
      live.commit === commit &&
      live.gitDirty === false &&
      live.projectNumber === 3 &&
      typeof live.bindingId === "string" &&
      live.bindingId.length > 0 &&
      typeof live.projectNodeId === "string" &&
      live.projectNodeId.length > 0 &&
      orderedItems.length > 0 &&
      orderedItems.every(
        (item) =>
          typeof item.projectItemNodeId === "string" &&
          item.projectItemNodeId.length > 0 &&
          typeof item.contentNodeId === "string" &&
          item.contentNodeId.length > 0,
      ) &&
      live.provenance?.host === "github.com" &&
      typeof live.provenance?.observedAt === "string" &&
      live.githubMutationCount === 0 &&
      typeof live.taskBytesBeforeSha256 === "string" &&
      live.taskBytesBeforeSha256 === live.taskBytesAfterSha256 &&
      typeof live.configBytesBeforeSha256 === "string" &&
      live.configBytesBeforeSha256 === live.configBytesAfterSha256 &&
      live.redacted === true;
    status = passed ? "pass" : "fail";
    evidence = passed
      ? `live Lucella Project 3 read passed at ${commit} with ${orderedItems.length} ordered memberships and zero writes`
      : "live probe JSON did not satisfy exact revision, identity, provenance, ordering, redaction, or no-write assertions";
    metric = {
      orderedMemberships: orderedItems.length,
      githubMutationCount: live.githubMutationCount,
      taskBytesUnchanged: live.taskBytesBeforeSha256 === live.taskBytesAfterSha256,
      configBytesUnchanged: live.configBytesBeforeSha256 === live.configBytesAfterSha256,
    };
  } else if (runner.executed && evidenceRead.error) {
    evidence = `${evidence}; ${evidenceRead.error}`;
  }
  criteria.push(
    resultObject(
      "MUST-014",
      "behavior",
      "An authenticated read-only Lucella Project 3 probe proves identity, membership order, provenance, freshness, and zero writes at the exact revision.",
      "api-roundtrip",
      status,
      evidence,
      metric,
    ),
  );
}

criteria.push(
  namedCriterion({
    id: "MUST-015",
    kind: "behavior",
    label: "Pre-decision task/config bytes, URL references, rollback, sb queue payload, and LangGraph custody remain compatible.",
    checks: [
      rustNamedTest(coreRunner(), "must_015_compatibility_and_rollback_preserve_native_work"),
      rustNamedTest(taskRunner(), "must_015_queue_protocol_remains_compatible"),
    ],
  }),
);

{
  const visualRead = readJsonEvidence(visualEvidencePath);
  const visual = visualRead.value;
  const shellCanonicalSha256 = sha256File(shellCanonicalPath);
  const bodyCanonicalSha256 = sha256File(bodyCanonicalPath);
  const requiredStates = [
    "loading",
    "fresh",
    "known-empty",
    "partial",
    "stale",
    "unavailable",
    "rate-limited",
    "duplicate-membership",
    "linked",
    "adopted",
    "long-title",
    "narrow-window",
    "100-of-500",
  ];
  const states = new Set(Array.isArray(visual?.states) ? visual.states : []);
  const findings = Array.isArray(visual?.findings) ? visual.findings : [];
  const shellEvidence = visual?.canonicals?.shell;
  const bodyEvidence = visual?.canonicals?.body;
  const bodyRevisionMatches = bodyCanonicalMatchesRevision(
    bodyEvidence?.revision,
    bodyCanonicalSha256,
  );
  const passed =
    visual !== null &&
    shellCanonicalSha256 !== null &&
    bodyCanonicalSha256 !== null &&
    visual.commit === commit &&
    visual.gitDirty === false &&
    visual.surface === "Tasks / Dispatches" &&
    requiredStates.every((state) => states.has(state)) &&
    shellEvidence?.path === "~/.lavish/switchbard-ia-places.html" &&
    shellEvidence?.sha256 === shellCanonicalSha256 &&
    typeof shellEvidence?.revision === "string" &&
    shellEvidence.revision.length > 0 &&
    bodyEvidence?.path === bodyCanonicalRelativePath &&
    bodyEvidence?.sha256 === bodyCanonicalSha256 &&
    bodyRevisionMatches &&
    bodyEvidence?.ownerReviewed === true &&
    typeof bodyEvidence?.reviewedAt === "string" &&
    typeof bodyEvidence?.reviewedBy === "string" &&
    bodyEvidence.reviewedBy.length > 0 &&
    visual.comparisons?.shell?.passed === true &&
    visual.comparisons?.body?.passed === true &&
    findings.every(
      (finding) =>
        finding.resolved === true &&
        (finding.resolutionRevision === commit || typeof finding.stableUrl === "string"),
    ) &&
    visual.humanApproval?.approved === true &&
    visual.humanApproval?.revision === commit &&
    typeof visual.humanApproval?.approver === "string" &&
    visual.humanApproval.approver.length > 0 &&
    typeof visual.humanApproval?.approvedAt === "string";
  criteria.push(
    resultObject(
      "MUST-016",
      "visual",
      "Every Task Queue state compares against the frozen IA V2 shell and owner-reviewed mixed-source body canonical at the exact clean revision.",
      "visual-review",
      passed ? "pass" : "blocked",
      passed
        ? `Visual Review state matrix matches both canonical hashes and human approval at clean revision ${commit}`
        : bodyCanonicalSha256 === null
          ? `required mixed-source body canonical ${bodyCanonicalRelativePath} is missing; TASK-80.4 UI implementation is blocked`
          : shellCanonicalSha256 === null
            ? "frozen IA V2 shell canonical ~/.lavish/switchbard-ia-places.html is missing"
            : visualRead.error ??
              "Visual Review evidence is incomplete, dirty, stale, unresolved, lacks canonical path/hash/revision proof, or lacks explicit human approval",
      visual === null
        ? null
        : {
            requiredStates: requiredStates.length,
            observedStates: states.size,
            findings: findings.length,
            approved: visual.humanApproval?.approved === true,
            shellCanonicalSha256,
            bodyCanonicalSha256,
            bodyRevisionMatches,
          },
    ),
  );
}

criteria.push(
  namedCriterion({
    id: "MUST-017",
    kind: "behavior",
    label: "Settings controls add, reorder, remove, and reload Project bindings while persisting exact source-band order.",
    checks: [
      rustNamedTest(guiRunner(), "must_017_settings_binding_controls_persist_order"),
    ],
  }),
);

const expectedIds = Array.from({ length: 17 }, (_, index) =>
  `MUST-${String(index + 1).padStart(3, "0")}`,
);
const actualIds = criteria.map((criterion) => criterion.id);
if (JSON.stringify(actualIds) !== JSON.stringify(expectedIds)) {
  throw new Error(`criterion ids do not match acceptance order: ${actualIds.join(", ")}`);
}

const mutationRead = readJsonEvidence(mutationEvidencePath);
const mutationDocument = mutationRead.value;
const suppliedMutations = Array.isArray(mutationDocument?.mutations)
  ? mutationDocument.mutations
  : [];
const mutationEligible = criteria.filter(
  (criterion) => criterion.kind === "behavior" || criterion.kind === "contract",
);
const mutationEntries = [];
const deferred = [];
const mutationProblems = [];

for (const criterion of mutationEligible) {
  if (criterion.status !== "pass") {
    deferred.push(criterion.id);
    continue;
  }
  const mutation = suppliedMutations.find(
    (entry) => entry.criterion_id === criterion.id || entry.criterionId === criterion.id,
  );
  const mutationFile = mutation?.mutation_file ?? mutation?.mutationFile;
  const productionPath =
    typeof mutationFile === "string" &&
    (mutationFile.startsWith("crates/") ||
      mutationFile.startsWith("orchestrator/switchbard_orchestrator/")) &&
    !mutationFile.includes("/tests/") &&
    !mutationFile.endsWith("verify.mjs");
  const valid =
    mutationDocument?.commit === commit &&
    mutation?.flipped === true &&
    mutation?.pass_to_fail === true &&
    mutation?.restored_pass === true &&
    productionPath &&
    typeof (mutation?.mutation_diff ?? mutation?.mutationDiff) === "string" &&
    typeof mutation?.evidence === "string";
  if (!valid) {
    mutationProblems.push(
      `${criterion.id} is green but lacks current-commit production-code PASS-to-FAIL-to-PASS mutation evidence`,
    );
    continue;
  }
  mutationEntries.push({
    criterion_id: criterion.id,
    mutation_file: mutationFile,
    mutation_diff: mutation.mutation_diff ?? mutation.mutationDiff,
    flipped: true,
    evidence: mutation.evidence,
  });
}

const mutationGate = {
  all_flipped: mutationProblems.length === 0,
  mutations: mutationEntries,
  deferred,
  evidence:
    mutationProblems.length === 0
      ? `${mutationEntries.length} green behavior/contract criteria mutation-probed; ${deferred.length} non-green criteria deferred`
      : mutationProblems.join("; "),
};

const artifacts = [
  "crates/switchbard-core/tests/task_queue_authority_contract.rs",
  "crates/switchbard-core/src/backlog/adoption.rs",
  "crates/switchbard-task/tests/task_queue_authority_contract.rs",
  "orchestrator/tests/test_task_queue_authority_contract.py",
  "crates/switchbard-gui/tests/task_queue_github_states.rs",
  "crates/switchbard-gui/tests/task_queue_github_perf_smoke.rs",
  "scripts/probe_task_queue_lucella.mjs",
  "tmp/task-queue-authority-model-live-lucella.json",
  "tmp/task-queue-authority-model-visual-review.json",
  "tmp/task-queue-authority-model-mutations.json",
  bodyCanonicalRelativePath,
  "tmp/task-queue-authority-model-acceptance.json",
].map((path) => {
  const absolute = repoPath(path);
  return {
    path,
    exists: path === "tmp/task-queue-authority-model-acceptance.json" || existsSync(absolute),
    modified_at:
      path === "tmp/task-queue-authority-model-acceptance.json" || !existsSync(absolute)
        ? null
        : statSync(absolute).mtime.toISOString(),
  };
});
artifacts.push({
  path: "~/.lavish/switchbard-ia-places.html",
  exists: existsSync(shellCanonicalPath),
  modified_at: existsSync(shellCanonicalPath)
    ? statSync(shellCanonicalPath).mtime.toISOString()
    : null,
});

const failures = criteria
  .filter((criterion) => criterion.status === "fail")
  .map((criterion) => ({ id: criterion.id, evidence: criterion.evidence }));
if (mutationProblems.length > 0) {
  failures.push({ id: "mutation-gate", evidence: mutationProblems.join("; ") });
}
const skipped = criteria
  .filter((criterion) => criterion.status === "skipped")
  .map((criterion) => criterion.id);
const blocked = criteria.filter((criterion) => criterion.status === "blocked");
const result =
  failures.length > 0
    ? "FAIL"
    : blocked.length > 0
      ? "BLOCKED"
      : skipped.length > 0
        ? "PARTIAL"
        : "PASS";

const metrics = Object.fromEntries(
  criteria
    .filter((criterion) => criterion.metric !== null)
    .map((criterion) => [criterion.id, criterion.metric]),
);

const summary = {
  plan: "task-queue-authority-model",
  result,
  commit,
  timestamp,
  criteria,
  metrics,
  artifacts,
  skipped,
  failures,
  mutation_gate: mutationGate,
};

writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");

process.stdout.write(`Task Queue authority model verifier: ${result}\n`);
for (const criterion of criteria) {
  process.stdout.write(
    `${criterion.status.toUpperCase().padEnd(7)} ${criterion.id} [${criterion.kind}] ${criterion.evidence}\n`,
  );
}
process.stdout.write(
  `Mutation gate: ${mutationEntries.length} probed, ${deferred.length} deferred, ${mutationProblems.length} invalid\n`,
);
process.stdout.write(`Summary: ${displayPath(summaryPath)}\n`);

process.exit(result === "PASS" ? 0 : 1);
