#!/usr/bin/env python3
"""Offline accounting for provider usage found in exported Orchestrate evidence.

This is an *offline analysis helper*. It reads raw provider transport that the
Build already retained (`build/artifacts/<action>/transport.jsonl` inside an
exported effort, a capture directory, or a capture ZIP) and reports
provider-reported token counts per action.

It never talks to a provider, never invents a number, and never converts tokens
into money or latency. Missing usage stays unknown. Provider-specific counter
semantics are version-scoped: the rules below describe the recorded Codex CLI
0.156.1 transport (`turn.completed.usage`), in which counters are cumulative for
the thread, `cached_input_tokens` is a subset of `input_tokens`, and
`reasoning_output_tokens` is a subset of `output_tokens`. Do not apply them to a
provider or version whose transport says something else.

Usage:

    python3 usage-accounting.py --evidence <exported-effort-dir-or-capture.zip>
    python3 usage-accounting.py --self-test

Deliberate limits:

* Cumulative counters are converted to per-action deltas by comparing a thread's
  observations in dispatch order, including across resumed controller runs.
* Identical observations copied into more than one capture are counted once.
* A decrease in a cumulative counter is reported as a reset and the new value is
  used as the baseline rather than a negative delta.
* Cache-read and reasoning tokens are subsets of the input and output totals.
  They are reported separately and never added on top.
* Actions with no usage record are reported as unknown, never as zero.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import zipfile
from pathlib import Path

# Transport fields this version-scoped rule set understands.  Anything else in
# an event is ignored rather than guessed at.
INPUT = "input_tokens"
CACHED_INPUT = "cached_input_tokens"
OUTPUT = "output_tokens"
REASONING_OUTPUT = "reasoning_output_tokens"
USAGE_FIELDS = (INPUT, CACHED_INPUT, OUTPUT, REASONING_OUTPUT)

THREAD_KEYS = ("thread_id", "session_id", "chat_id", "threadId", "sessionId", "chatId")


def observations(paths):
    """Every parseable usage observation, in the order the evidence lists it.

    A file is treated as a copy only when the *same action* directory appears
    twice with identical bytes, which is what a copied capture looks like.  Two
    different actions that happen to emit identical traffic are two actions.
    """
    seen_files = set()
    for source, text, order in paths:
        digest = hashlib.sha256(text.encode("utf-8", "replace")).hexdigest()
        action = Path(source.split("!")[-1]).parent.name or source
        duplicated_file = (action, digest) in seen_files
        seen_files.add((action, digest))
        for line_number, line in enumerate(text.splitlines(), start=1):
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if not isinstance(event, dict):
                continue
            usage = event.get("usage")
            if not isinstance(usage, dict):
                continue
            thread = None
            for key in THREAD_KEYS:
                if isinstance(event.get(key), str):
                    thread = event[key]
                    break
            yield {
                "source": source,
                "line": line_number,
                "order": order,
                "thread": thread,
                "usage": {
                    field: usage.get(field) if isinstance(usage.get(field), int) else None
                    for field in USAGE_FIELDS
                },
                "duplicate_file": duplicated_file,
            }


def deltas(observations, cumulative=True):
    """Per-thread deltas from cumulative counters, with resets and dupes marked."""
    baselines = {}
    results = []
    suppressed = 0
    for observation in observations:
        thread = observation["thread"] or "<no-thread>"
        usage = observation["usage"]
        previous = baselines.get(thread)
        identity = (thread, observation["line"], tuple(sorted(usage.items())))
        if observation["duplicate_file"] and previous is not None and previous["identity"] == identity:
            suppressed += 1
            continue
        delta = {}
        flags = []
        if cumulative:
            for field, value in usage.items():
                if value is None:
                    delta[field] = None
                    continue
                before = None if previous is None else previous["usage"].get(field)
                if before is None:
                    delta[field] = value
                elif value >= before:
                    delta[field] = value - before
                else:
                    delta[field] = value
                    flags.append(f"reset:{field}")
        else:
            delta = dict(usage)
        baselines[thread] = {
            "usage": {field: value for field, value in usage.items() if value is not None},
            "identity": identity,
        }
        results.append(
            {
                "thread": thread,
                "source": observation["source"],
                "flags": flags,
                "delta": delta,
                "cumulative": usage,
            }
        )
    return results, suppressed


def summarize(results, suppressed):
    total = {field: 0 for field in (INPUT, OUTPUT)}
    unknown = 0
    resets = 0
    for result in results:
        delta = result["delta"]
        if delta.get(INPUT) is None and delta.get(OUTPUT) is None:
            unknown += 1
        else:
            # Only the headline counters are summed.  Cache reads and reasoning
            # tokens are subsets of these totals and are never added on top.
            total[INPUT] += delta.get(INPUT) or 0
            total[OUTPUT] += delta.get(OUTPUT) or 0
        resets += sum(1 for flag in result["flags"] if flag.startswith("reset:"))
    return {
        "observations": len(results),
        "duplicate_observations_suppressed": suppressed,
        "actions_without_usage": unknown,
        "counter_resets": resets,
        "total_input_tokens": total[INPUT],
        "total_output_tokens": total[OUTPUT],
        "note": (
            "Provider-reported token counts only. This is not a cost, a billing "
            "figure, a latency measurement, or a claim about subscription usage; "
            "cache-read and reasoning tokens are subsets of the totals above and "
            "are reported per observation, never added twice."
        ),
    }


def collect_paths(target):
    """Evidence files from a directory, an exported effort, or a capture ZIP."""
    target = Path(target)
    sources = []
    if target.is_dir():
        candidates = sorted(target.rglob("transport.jsonl"))
    elif target.suffix == ".zip":
        candidates = []
        with zipfile.ZipFile(target) as archive:
            for name in sorted(archive.namelist()):
                if name.endswith("transport.jsonl"):
                    candidates.append(name)
            return [
                (
                    f"{target}!{name}",
                    archive.read(name).decode("utf-8", "replace"),
                    index,
                )
                for index, name in enumerate(candidates)
            ]
    else:
        raise SystemExit(f"evidence target is neither a directory nor a ZIP: {target}")
    for index, path in enumerate(candidates):
        sources.append(
            (str(path), path.read_text(encoding="utf-8", errors="replace"), index)
        )
    return sources


def self_test():
    """Fixtures for every counter semantic the guidance must distinguish."""
    failures = []

    def check(name, actual, expected):
        if actual != expected:
            failures.append(f"{name}: expected {expected!r}, got {actual!r}")

    def run(events, cumulative=True):
        text = "\n".join(json.dumps(event) for event in events) + "\n"
        return deltas(observations([("fixture", text, 0)]), cumulative=cumulative)

    # 1. Cumulative counters over one thread become per-observation deltas.
    results, _ = run(
        [
            {"thread_id": "t1", "usage": {INPUT: 100, OUTPUT: 10}},
            {"thread_id": "t1", "usage": {INPUT: 240, OUTPUT: 25}},
            {"thread_id": "t1", "usage": {INPUT: 400, OUTPUT: 41}},
        ]
    )
    check("cumulative deltas", [r["delta"][INPUT] for r in results], [100, 140, 160])
    check("cumulative totals", summarize(results, 0)["total_input_tokens"], 400)

    # 2. Per-turn counters are used as they are.
    results, _ = run(
        [
            {"thread_id": "t2", "usage": {INPUT: 100, OUTPUT: 10}},
            {"thread_id": "t2", "usage": {INPUT: 90, OUTPUT: 9}},
        ],
        cumulative=False,
    )
    check("per-turn deltas", [r["delta"][INPUT] for r in results], [100, 90])

    # 3. A copied capture does not duplicate consumption: the same action's
    #    identical transport appearing twice counts once.
    text = json.dumps({"thread_id": "t3", "usage": {INPUT: 100, OUTPUT: 10}}) + "\n"
    results, suppressed = deltas(
        observations(
            [
                ("capture1/artifacts/act-1/transport.jsonl", text, 0),
                ("capture2/artifacts/act-1/transport.jsonl", text, 1),
            ]
        )
    )
    check("copied capture deltas", [r["delta"][INPUT] for r in results], [100])
    check("copied capture totals", summarize(results, suppressed)["total_input_tokens"], 100)
    check("copied capture suppression", suppressed, 1)
    # Two distinct actions with identical traffic are still two actions.
    results, suppressed = deltas(
        observations(
            [
                ("capture/artifacts/act-1/transport.jsonl", text, 0),
                ("capture/artifacts/act-2/transport.jsonl", text, 1),
            ]
        )
    )
    check("distinct actions counted", summarize(results, suppressed)["observations"], 2)

    # 4. A counter reset is visible and does not produce a negative delta.
    results, _ = run(
        [
            {"thread_id": "t4", "usage": {INPUT: 100, OUTPUT: 10}},
            {"thread_id": "t4", "usage": {INPUT: 50, OUTPUT: 4}},
        ]
    )
    check("reset delta", results[1]["delta"][INPUT], 50)
    check("reset flagged", "reset:input_tokens" in results[1]["flags"], True)
    check("reset counted", summarize(results, 0)["counter_resets"], 2)

    # 5. Missing usage stays unknown, not zero.
    results, _ = run([{"thread_id": "t5", "usage": {}}])
    check("missing usage unknown", summarize(results, 0)["actions_without_usage"], 1)
    check("missing usage total", summarize(results, 0)["total_input_tokens"], 0)

    # 6. Cache and reasoning subsets are reported, never added twice.
    results, _ = run(
        [
            {
                "thread_id": "t6",
                "usage": {
                    INPUT: 100,
                    CACHED_INPUT: 40,
                    OUTPUT: 20,
                    REASONING_OUTPUT: 5,
                },
            }
        ]
    )
    summary = summarize(results, 0)
    check("subset totals", (summary["total_input_tokens"], summary["total_output_tokens"]), (100, 20))
    check("subset reported separately", results[0]["delta"][CACHED_INPUT], 40)

    # 7. A resumed session that continues its cumulative series is not restarted.
    text_first = json.dumps({"thread_id": "t7", "usage": {INPUT: 100, OUTPUT: 10}}) + "\n"
    text_second = json.dumps({"thread_id": "t7", "usage": {INPUT: 260, OUTPUT: 30}}) + "\n"
    results, _ = deltas(observations([("run1", text_first, 0), ("run2", text_second, 1)]))
    check("resumed deltas", [r["delta"][INPUT] for r in results], [100, 160])

    if failures:
        for failure in failures:
            print(f"FAIL {failure}", file=sys.stderr)
        raise SystemExit(1)
    print("usage-accounting self-test: all fixtures behave as documented")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", help="exported effort directory or capture ZIP")
    parser.add_argument(
        "--per-turn",
        action="store_true",
        help="treat counters as per-request instead of the codex 0.156.1 cumulative form",
    )
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--json", action="store_true", help="print machine-readable output")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.evidence:
        parser.error("provide --evidence or --self-test")
    paths = collect_paths(args.evidence)
    if not paths:
        raise SystemExit(f"no transport.jsonl evidence found under {args.evidence}")
    results, suppressed = deltas(
        observations(paths), cumulative=not args.per_turn
    )
    summary = summarize(results, suppressed)
    if args.json:
        print(json.dumps({"summary": summary, "observations": results}, indent=2))
        return
    print(
        f"observations: {summary['observations']} "
        f"(duplicates suppressed: {summary['duplicate_observations_suppressed']})"
    )
    print(f"counter resets: {summary['counter_resets']}")
    print(f"actions without usage (unknown, not zero): {summary['actions_without_usage']}")
    print(f"input tokens (delta sum): {summary['total_input_tokens']}")
    print(f"output tokens (delta sum): {summary['total_output_tokens']}")
    print(summary["note"])


if __name__ == "__main__":
    main()
