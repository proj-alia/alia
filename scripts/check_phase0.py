#!/usr/bin/env python3
"""Validate M0 artifacts, not the unimplemented companion runtime."""

import copy
import hashlib
import json
import math
from pathlib import Path
import re
import sys
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

from jsonschema import Draft202012Validator, FormatChecker, ValidationError

ROOT = Path(__file__).resolve().parents[1]
CAPABILITIES = {"memory.search", "documents.read", "notes.write", "web.read_allowed"}
COUNTS = {}


def check(condition, message, category):
    if not condition:
        raise AssertionError(message)
    COUNTS[category] = COUNTS.get(category, 0) + 1


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def reject_constant(value):
    raise ValueError(f"nonfinite JSON number: {value}")


def parse(text):
    def finite_float(value):
        parsed = float(value)
        if not math.isfinite(parsed):
            raise ValueError("nonfinite JSON number")
        return parsed
    return json.loads(text, object_pairs_hook=unique_object,
                      parse_constant=reject_constant, parse_float=finite_float)


def read_json(path):
    return parse((ROOT / path).read_text(encoding="utf-8"))


def closed_objects(node):
    if isinstance(node, dict):
        if node.get("type") == "object":
            check(node.get("additionalProperties") is False,
                  "schema object permits unknown fields", "closed_objects")
        for child in node.values():
            closed_objects(child)
    elif isinstance(node, list):
        for child in node:
            closed_objects(child)


def validate(name, value, validators, *, available=None, stage="M3"):
    validators[name].validate(value)
    if name == "runtime":
        budget = value["budgets"]
        if budget["background_tokens_per_day"] > budget["total_tokens_per_day"]:
            raise ValueError("background budget exceeds total")
        try:
            ZoneInfo(value["contact"]["timezone"])
        except (ZoneInfoNotFoundError, ValueError) as error:
            raise ValueError("unknown IANA timezone") from error
        available = CAPABILITIES if available is None else available
        if not set(value["capabilities"]["enabled"]) <= available:
            raise ValueError("configured capability is not implemented")
        for host in value["capabilities"]["allowed_web_hosts"]:
            if host in {"localhost", "localhost.localdomain"} or host.endswith(".localhost"):
                raise ValueError("loopback hostname is forbidden")
    if name == "persona":
        ids = [item["id"] for item in value["personality"]["values"]]
        if len(ids) != len(set(ids)):
            raise ValueError("duplicate value identifiers")
    if name == "decision" and stage == "M1" and value["development"] is not None:
        raise ValueError("unsupported_proposal_field")


def must_reject(label, callback):
    try:
        callback()
    except (ValidationError, ValueError):
        check(True, label, "negative_cases")
        return
    raise AssertionError(f"accepted invalid input: {label}")


def mutate(value, path, replacement):
    value = copy.deepcopy(value)
    cursor = value
    for key in path[:-1]:
        cursor = cursor[key]
    cursor[path[-1]] = replacement
    return value


def main():
    schemas = {name: read_json(f"schemas/{name}.schema.json")
               for name in ("persona", "runtime", "decision", "goal-assessment", "decision-assessment", "privacy-operation", "development-revert")}
    validators = {}
    for name, schema in schemas.items():
        Draft202012Validator.check_schema(schema)
        check(True, name, "schemas")
        closed_objects(schema)
        validators[name] = Draft202012Validator(schema, format_checker=FormatChecker())
    samples = {
        "persona": read_json("config/persona.example.json"),
        "runtime": read_json("config/runtime.example.json"),
        "decision": read_json("evals/decision.example.json"),
        "goal-assessment": read_json("evals/goal-assessment.example.json"),
        "decision-assessment": read_json("evals/decision-assessment.example.json"),
        "privacy-operation": read_json("evals/privacy-operation.example.json"),
        "development-revert": read_json("evals/development-revert.example.json"),
    }
    for name, sample in samples.items():
        validate(name, sample, validators)
        check(True, name, "examples")

    for bad in ({}, {"reason": None}, {"reason": ""}, {"reason": " \n\t"},
                {"reason": "a\0b"}, {"reason": "界" * 1001},
                {"reason": "x", "approved": True}, {"reason": "x", "state_version": 1}):
        must_reject("strict development revert request", lambda: validate("development-revert", bad, validators))
    validate("development-revert", {"reason": "界" * 1000}, validators)
    check(True, "Unicode development reason boundary", "examples")

    progress = dict(samples["privacy-operation"], observed_at="2026-11-12T00:00:00Z", last_error=None)
    validate("privacy-operation", progress, validators)
    check(True, "authenticated deletion progress (schema only)", "examples")
    for path, replacement in [
        (("status",), "approved"), (("state_version",), 0), (("privacy_epoch",), -1),
        (("operation_id",), "not-a-uuid"), (("target_id",), "00000000-0000-0000-0000-000000000000"),
        (("raw_key",), "must-not-be-in-response"),
    ]:
        bad = mutate(samples["privacy-operation"], path, replacement)
        must_reject("invalid deletion response", lambda: validate("privacy-operation", bad, validators))
    bad = dict(progress, status="completed", last_error="cleanup_failed")
    must_reject("failed cleanup cannot be completed", lambda: validate("privacy-operation", bad, validators))

    autobiography_decision = read_json("evals/autobiography-decision.example.json")
    validate("decision", autobiography_decision, validators)
    check(True, "source-bound historical interpretation (schema only)", "examples")
    for path, replacement in [
        (("autobiography_changes",0,"permissions"),["unlimited"]),
        (("autobiography_changes",0,"topic"),"../identity"),
        (("autobiography_changes",0,"event_ids"),[]),
        (("autobiography_changes",0,"belief_ids"),["not-a-uuid"]),
        (("autobiography_changes",),autobiography_decision["autobiography_changes"]*3),
    ]:
        bad = mutate(autobiography_decision,path,replacement)
        must_reject("invalid autobiography shape",lambda:validate("decision",bad,validators))

    social_decision=read_json("evals/social-decision.example.json")
    validate("decision",social_decision,validators)
    check(True,"source-bound social changes (schema only)","examples")
    for path,replacement in [
        (("appraisal_changes",0,"grant_permission"),True),
        (("appraisal_changes",0,"valence"),1.1),
        (("appraisal_changes",0,"event_ids"),[]),
        (("appraisal_changes",),social_decision["appraisal_changes"]*2),
        (("attitude_changes",0,"topic"),"../identity"),
        (("attitude_changes",0,"event_ids"),["not-a-uuid"]),
        (("relationship_changes",0,"expected_relationship_version"),0),
        (("relationship_changes",0,"changes","permissions"),["all"]),
        (("relationship_changes",0,"changes"),{}),
        (("relationship_changes",),social_decision["relationship_changes"]*3),
    ]:
        bad=mutate(social_decision,path,replacement)
        must_reject("invalid social change shape",lambda:validate("decision",bad,validators))

    belief_decision = read_json("evals/belief-decision.example.json")
    validate("decision", belief_decision, validators)
    check(True, "evidence-bound belief changes (schema only)", "examples")
    for path, replacement in [
        (("belief_changes", 0, "permissions"), ["unlimited"]),
        (("belief_changes", 0, "position"), "verified_truth"),
        (("belief_changes", 0, "counter_event_ids"), ["not-an-event-uuid"]),
        (("belief_changes",), belief_decision["belief_changes"] * 4),
    ]:
        bad = mutate(belief_decision, path, replacement)
        must_reject("invalid belief shape", lambda: validate("decision", bad, validators))

    typed_goal = read_json("evals/goal-origins.example.json")
    validate("decision", typed_goal, validators)
    check(True, "typed goal origins (schema only)", "examples")
    for path, replacement in [
        (("goal_changes",0,"origins",0,"origin_type"),"unknown"),
        (("goal_changes",0,"origins",0,"origin_id"),"not-a-uuid"),
        (("goal_changes",0,"origins",0,"label"),"not a reference field"),
        (("goal_changes",0,"origins"),[typed_goal["goal_changes"][0]["origins"][0]]*2),
        (("goal_changes",0,"origin_type"),"event"),
    ]:
        bad=mutate(typed_goal,path,replacement)
        must_reject("invalid typed goal reference",lambda:validate("decision",bad,validators))

    cases = [
        ("goal-assessment", ("goal_revision",), 0),
        ("goal-assessment", ("tool_intents",), []),
        ("goal-assessment", ("verdict",), "met"),
        ("goal-assessment", ("unmet_conditions",), []),
        ("decision-assessment", ("checks",), []),
        ("decision-assessment", ("checks", 0, "verdict"), "always_trust"),
        ("decision-assessment", ("self_grant_permissions",), True),
        ("persona", ("unknown",), True),
        ("persona", ("schema_version",), 2),
        ("persona", ("identity", "private_memories"), ["invented"]),
        ("persona", ("personality", "traits", "curiosity"), 1.1),
        ("persona", ("relationship", "assume_intimacy"), True),
        ("runtime", ("autonomy", "max_model_calls_per_cycle"), 4),
        ("runtime", ("autonomy", "max_tool_calls_per_cycle"), 0),
        ("runtime", ("budgets", "background_tokens_per_day"), 200001),
        ("runtime", ("contact", "timezone"), "Mars/Olympus"),
        ("runtime", ("contact", "quiet_start"), "25:00"),
        ("runtime", ("development", "allow_value_change"), False),
        ("runtime", ("capabilities", "enabled"), ["shell.execute"]),
        ("runtime", ("capabilities", "self_grant_permissions"), True),
        ("runtime", ("capabilities", "allowed_web_hosts"), ["*.example.com"]),
        ("runtime", ("capabilities", "allowed_web_hosts"), ["127.0.0.1"]),
        ("runtime", ("capabilities", "allowed_web_hosts"), ["localhost"]),
        ("decision", ("memory_changes", 0, "source_event_ids"), []),
        ("decision", ("reply_evidence_ids",), ["not-a-uuid"]),
        ("decision", ("tool_intents",), [{"capability_id": "shell.execute", "input": {}}]),
        ("decision", ("goal_changes",), [{"action": "complete", "goal_id": "missing"}]),
        ("decision", ("hidden_reasoning",), "must not store"),
        ("decision", ("tool_intents",), [{"capability_id": "notes.write", "input": {"title": "x", "text": "x", "path": "/etc/passwd"}}]),
    ]
    for tool in (
        {"capability_id": "documents.read", "input": {"document_id": "00000000-0000-4000-8000-000000000001", "offset": -1}},
        {"capability_id": "documents.read", "input": {"document_id": "00000000-0000-4000-8000-000000000001", "max_chars": 4097}},
        {"capability_id": "memory.search", "input": {"query": "资料", "limit": 6}},
        {"capability_id": "memory.search", "input": {"query": "偏好", "person_id": "not-a-uuid"}},
        {"capability_id": "memory.search", "input": {"query": "偏好", "person_name": "甲"*81}},
        {"capability_id": "memory.search", "input": {"query": "偏好", "person_name": "林澄", "person_id": "00000000-0000-4000-8000-000000000001"}},
    ):
        cases.append(("decision", ("tool_intents",), [tool]))
    tool_example = read_json("evals/tools.example.json")
    validate("decision", tool_example, validators)
    check(True, "bounded local tool inputs", "examples")
    completion_example = read_json("evals/completion.example.json")
    validate("decision", completion_example, validators)
    check(True, "explicit excerpt completion contract", "examples")
    for field, value in (("kind", "any_goal"), ("document_id", "invalid"), ("start_char", -1), ("extra", True)):
        bad = copy.deepcopy(completion_example)
        bad["goal_changes"][0]["completion"][field] = value
        must_reject("invalid completion contract field", lambda: validate("decision", bad, validators))
    for name, path, replacement in cases:
        value = mutate(samples[name], path, replacement)
        must_reject(f"{name}:{path}", lambda: validate(name, value, validators))
    duplicate_values = copy.deepcopy(samples["persona"])
    duplicate_values["personality"]["values"][1]["id"] = duplicate_values["personality"]["values"][0]["id"]
    must_reject("duplicate value ids", lambda: validate("persona", duplicate_values, validators))
    must_reject("unimplemented capability", lambda: validate("runtime", samples["runtime"], validators, available=set()))
    for text in ('{"x":1,"x":2}', '{"x":NaN}', '{"x":Infinity}', '{"x":1e999}'):
        must_reject("invalid JSON", lambda: parse(text))

    future = copy.deepcopy(samples["decision"])
    future["development"] = {
        "target": "persona.values", "before_version": 1,
        "proposed_value": samples["persona"]["personality"]["values"],
        "proposed_description": "在重要判断中提高证据核查的优先级",
        "evidence_ids": ["00000000-0000-4000-8000-000000000001"],
        "counterevidence_ids": [], "validation_plan": "在相关与无关场景试用并比较",
        "deadline": "2026-12-01T00:00:00Z",
    }
    validate("decision", future, validators, stage="M3")
    check(True, "M3 future proposal shape", "examples")
    must_reject("M1 cannot execute M3 proposal", lambda: validate("decision", future, validators, stage="M1"))
    must_reject("development cannot modify permissions", lambda: validate(
        "decision", mutate(future, ("development", "target"), "permissions"), validators, stage="M3"))

    docs = {name: (ROOT / f"docs/{name}.md").read_text() for name in ("BLUEPRINT", "CONTRACTS", "IMPLEMENTATION")}
    scenarios = read_json("evals/scenarios.json")
    expected_ids = {f"AT-{i:02d}" for i in range(1, 19)}
    ids = [row["id"] for row in scenarios["scenarios"]]
    check(len(ids) == len(set(ids)) and set(ids) == expected_ids, "18 unique scenarios required", "traceability")
    for row in scenarios["scenarios"]:
        check(row["status"] == "not_run" and bool(row["title"]), "spec is not an execution report", "traceability")
        check(bool(row["stages"]) and set(row["stages"]) <= {f"M{i}" for i in range(1, 6)}, "invalid milestone", "traceability")
        for key in ("preconditions", "steps", "assertions", "evidence_required"):
            check(isinstance(row[key], list) and bool(row[key]) and all(isinstance(v, str) and v.strip() for v in row[key]), f"{row['id']} missing {key}", "traceability")
    referenced = set(re.findall(r"AT-\d{2}", "\n".join(docs.values())))
    check(referenced == expected_ids, "document/scenario IDs disagree", "traceability")
    task_rows = re.findall(r"^\| (M[1-5]-\d{2}) \| (M\d(?:-\d{2})?) \|", docs["IMPLEMENTATION"], re.M)
    tasks = dict(task_rows)
    check(len(tasks) == len(task_rows) == 16, "expected 16 unique implementation tasks", "traceability")
    for task in tasks:
        seen = set()
        current = task
        while current != "M0":
            check(current in tasks and current not in seen, f"broken/cyclic dependency from {task}", "task_edges")
            seen.add(current)
            current = tasks[current]

    paths = [ROOT / "README.md", ROOT / "AGENTS.md"] + list((ROOT / "docs").glob("*.md"))
    for path in paths:
        content = path.read_text(encoding="utf-8")
        check(all(line == line.rstrip() for line in content.splitlines()), f"trailing whitespace: {path}", "documents")
        for target in re.findall(r"\[[^\]]*\]\(([^\s)]+)\)", content):
            if re.match(r"[a-z]+://", target) or target.startswith("#"):
                continue
            target = target.split("#")[0]
            check((path.parent / target).is_file(), f"broken local link: {path} -> {target}", "links")
    hashes = {
        "contexts/01.md": "f9de421a925e2a17d13b521e2e252d57e492257a1d0265504c1bf2efd589e81c",
        "contexts/02.md": "c92be233a43212402401061e82e5b36e7359a22af60e5d35dfef04580658c4b4",
    }
    for path, expected in hashes.items():
        check(hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == expected, f"original source changed: {path}", "sources")
    print("M0 PASS: " + ", ".join(f"{key}={value}" for key, value in COUNTS.items()))
    print("This checker does not execute product acceptance scenarios or the 14-day observation; see IMPLEMENTATION.md for separate engineering results.")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, ValueError, KeyError, OSError, ValidationError) as error:
        print(f"M0 FAIL: {error}", file=sys.stderr)
        sys.exit(1)
