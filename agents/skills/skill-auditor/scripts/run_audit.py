#!/usr/bin/env python3
"""Coordinate provider-neutral skill-auditor runs."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import collect_skills
import collect_transcripts
import generate_report


DEFAULT_DAYS = 14
DEFAULT_MAX_BATCHES = 12
DEFAULT_BATCH_SIZE = 60


@dataclass(frozen=True, slots=True)
class RunConfig:
    provider: str
    project_path: str
    days: int
    min_turns: int
    language: str
    base_dir: Path | None
    workspace: Path | None
    batch_size: int
    max_batches: int


def default_base_dir(provider: str, project_path: str) -> Path:
    """Return the default report base directory for a provider and scope."""
    provider_root = Path(f"~/.{provider}/skill-report").expanduser()
    if project_path == "all":
        return provider_root / "all"
    project_root = Path(project_path).expanduser().resolve()
    report_root = find_repo_root(project_root)
    return provider_root / "projects" / project_slug(report_root, project_root)


def find_repo_root(path: Path) -> Path:
    """Return the nearest git worktree root, or the input path when unavailable."""
    current = path if path.is_dir() else path.parent
    for candidate in (current, *current.parents):
        if (candidate / ".git").exists():
            return candidate
    return current


def project_slug(report_root: Path, project_root: Path) -> str:
    """Return a stable directory slug for a project inside the report root."""
    try:
        relative = project_root.relative_to(report_root)
    except ValueError:
        relative = Path(project_root.name)
    parts = [report_root.parent.name, report_root.name]
    if str(relative) not in ("", "."):
        parts.extend(relative.parts)
    return "-".join(part for part in parts if part not in ("", "."))


def timestamped_workspace(base_dir: Path) -> Path:
    run_id = datetime.now().strftime("%Y-%m-%dT%H-%M-%S")
    return base_dir / run_id


def prepare_run(config: RunConfig) -> Path:
    """Collect inputs and write sub-agent prompt files."""
    workspace = config.workspace or timestamped_workspace(
        config.base_dir or default_base_dir(config.provider, config.project_path)
    )
    workspace.mkdir(parents=True, exist_ok=True)

    transcripts = collect_transcripts.collect(
        config.project_path,
        days=config.days,
        min_turns=config.min_turns,
        provider=config.provider,
    )
    if "error" in transcripts:
        raise RuntimeError(json.dumps(transcripts, indent=2, ensure_ascii=False))

    manifest = collect_skills.collect(provider=config.provider)
    write_json(workspace / "transcripts.json", transcripts)
    write_json(workspace / "skill_manifest.json", manifest)

    batches = build_batches(
        transcripts,
        manifest,
        batch_size=config.batch_size,
        max_batches=config.max_batches,
    )
    write_json(workspace / "batches.json", batches)
    write_agent_prompts(workspace, batches, config.language)

    print_collection_summary(transcripts, manifest, batches, workspace)
    return workspace


def build_batches(
    transcripts: dict,
    manifest: dict,
    batch_size: int = DEFAULT_BATCH_SIZE,
    max_batches: int = DEFAULT_MAX_BATCHES,
) -> list[dict]:
    """Build project-aware routing audit batches."""
    sessions = transcripts.get("sessions", [])
    skills = manifest.get("skills", [])
    global_names = [s["name"] for s in skills if s.get("scope") == "global"]
    dmi_names = {s["name"] for s in skills if s.get("disable_model_invocation")}

    global_only_indices: list[int] = []
    local_project_groups: dict[str, list[int]] = {}

    for i, session in enumerate(sessions):
        project_dir = session.get("project_dir") or "unknown"
        local_names = local_skill_names(project_dir, skills)
        if local_names:
            local_project_groups.setdefault(project_dir, []).append(i)
        else:
            global_only_indices.append(i)

    batches: list[dict] = []
    for chunk in chunks(global_only_indices, batch_size):
        batches.append(
            {
                "session_indices": chunk,
                "label": "global-only (mixed projects)",
                "visible_skill_names": global_names,
            }
        )

    by_skill_set: dict[tuple[str, ...], list[int]] = {}
    for project_dir, indices in local_project_groups.items():
        names = tuple(local_skill_names(project_dir, skills))
        by_skill_set.setdefault(names, []).extend(indices)

    local_batches = []
    for names, indices in by_skill_set.items():
        for chunk in chunks(indices, batch_size):
            local_batches.append(
                {
                    "session_indices": chunk,
                    "label": local_label("local skills", names),
                    "visible_skill_names": global_names + list(names),
                    "_local_set": set(names),
                }
            )

    remaining_budget = max_batches - len(batches)
    while len(local_batches) > remaining_budget and len(local_batches) > 1:
        smallest_idx = min(
            range(len(local_batches)),
            key=lambda i: len(local_batches[i]["session_indices"]),
        )
        smallest = local_batches.pop(smallest_idx)
        best_idx = most_similar_batch_index(smallest, local_batches)
        target = local_batches[best_idx]
        target["session_indices"].extend(smallest["session_indices"])
        target["_local_set"] = target["_local_set"] | smallest["_local_set"]
        merged_names = tuple(sorted(target["_local_set"]))
        target["visible_skill_names"] = global_names + list(merged_names)
        target["label"] = local_label("merged local skills", merged_names)

    for batch in local_batches:
        batch.pop("_local_set", None)
        batches.append(batch)

    for i, batch in enumerate(batches):
        batch["batch_index"] = i
        batch["dmi_skill_names"] = sorted(set(batch["visible_skill_names"]) & dmi_names)

    return batches


def local_skill_names(project_dir: str, skills: list[dict]) -> list[str]:
    names = []
    for skill in skills:
        if skill.get("scope") != "project-local" or not skill.get("project_path"):
            continue
        project_path = str(skill["project_path"])
        encoded = project_path.replace("/", "-").replace(".", "-")
        if project_dir == project_path or encoded.lstrip("-") in project_dir.lstrip("-"):
            names.append(skill["name"])
    return sorted(names)


def chunks(items: list[int], size: int) -> list[list[int]]:
    return [items[i : i + size] for i in range(0, len(items), size)]


def local_label(prefix: str, names: tuple[str, ...]) -> str:
    preview = ", ".join(names[:3])
    suffix = "..." if len(names) > 3 else ""
    return f"{prefix}: {preview}{suffix}"


def most_similar_batch_index(smallest: dict, batches: list[dict]) -> int:
    best_idx = 0
    best_extra = float("inf")
    for i, batch in enumerate(batches):
        extra = len(smallest["_local_set"] - batch["_local_set"])
        extra += len(batch["_local_set"] - smallest["_local_set"])
        if extra < best_extra:
            best_idx = i
            best_extra = extra
    return best_idx


def write_agent_prompts(workspace: Path, batches: list[dict], language: str) -> None:
    prompt_dir = workspace / "agent-prompts"
    prompt_dir.mkdir(exist_ok=True)

    for batch in batches:
        i = batch["batch_index"]
        prompt = routing_prompt(workspace, batch, language)
        (prompt_dir / f"routing_batch_{i}.md").write_text(prompt, encoding="utf-8")

    (prompt_dir / "portfolio_analysis.md").write_text(
        portfolio_prompt(workspace, language),
        encoding="utf-8",
    )
    (prompt_dir / "improvement_plan.md").write_text(
        improvement_prompt(workspace, language),
        encoding="utf-8",
    )


def routing_prompt(workspace: Path, batch: dict, language: str) -> str:
    return f"""Write all human-readable output text in {language}.

Read agents/routing_analyst.md from the skill-auditor skill directory for your
analysis instructions.
Read {workspace}/skill_manifest.json for skill definitions.
Read {workspace}/transcripts.json for session data.
Only analyze sessions with these indices: {batch["session_indices"]}.
Only evaluate against these skills: {batch["visible_skill_names"]}.
Ignore skills not in this list; they are not available in this project context.
These skills have disable-model-invocation: true and NEVER auto-fire:
{batch["dmi_skill_names"]}. Do NOT flag them as false_negative.
Write your analysis as JSON to {workspace}/batch_audit_{batch["batch_index"]}.json
following the exact schema in schemas/schemas.md (audit_report.json section).
"""


def portfolio_prompt(workspace: Path, language: str) -> str:
    return f"""Write all human-readable output text in {language}.

Read agents/portfolio_analyst.md from the skill-auditor skill directory.
Read {workspace}/skill_manifest.json for skill definitions and attention budget.
Read {workspace}/audit_report.json for the routing audit results.
Write your portfolio analysis as JSON to {workspace}/portfolio_analysis.json.
"""


def improvement_prompt(workspace: Path, language: str) -> str:
    return f"""Write all human-readable output text in {language}.

Read agents/improvement_planner.md from the skill-auditor skill directory.
Read {workspace}/audit_report.json for routing audit results.
Read {workspace}/portfolio_analysis.json for portfolio analysis.
Read {workspace}/skill_manifest.json for current skill definitions.
Write your improvement proposals as JSON to {workspace}/improvement_proposals.json.
Also write individual patch files to {workspace}/patches/ directory.
"""


def merge_workspace(workspace: Path) -> Path:
    """Merge batch audit files into audit_report.json."""
    batch_files = sorted(workspace.glob("batch_audit_*.json"))
    if not batch_files:
        raise RuntimeError(f"No batch_audit_*.json files found in {workspace}")

    reports = [read_json_dict(path) for path in batch_files]
    merged = merge_audit_reports(reports)
    output = workspace / "audit_report.json"
    write_json(output, merged)
    return output


def merge_audit_reports(reports: list[dict]) -> dict:
    by_skill: dict[str, dict] = {}
    never_fired: dict[str, dict] = {}
    competition_pairs: dict[tuple[str, str], dict] = {}
    coverage_gaps: dict[str, dict] = {}
    meta: dict[str, int | str] = {
        "sessions_analyzed": 0,
        "turns_analyzed": 0,
        "turns_with_skill_activity": 0,
        "turns_no_skill_needed": 0,
        "skills_in_scope": 0,
        "analyzed_at": datetime.now(timezone.utc).isoformat(),
    }

    for report in reports:
        for skill_report in report.get("skill_reports", []):
            merge_skill_report(by_skill, skill_report)
        for item in report.get("skills_never_fired", []):
            never_fired.setdefault(item.get("skill_name", ""), item)
        for item in report.get("competition_pairs", []):
            key = tuple(sorted((item.get("skill_a", ""), item.get("skill_b", ""))))
            merge_competition_pair(competition_pairs, key, item)
        for item in report.get("coverage_gaps", []):
            merge_coverage_gap(coverage_gaps, item)
        for key in (
            "sessions_analyzed",
            "turns_analyzed",
            "turns_with_skill_activity",
            "turns_no_skill_needed",
        ):
            meta[key] = int(meta[key]) + int(report.get("meta", {}).get(key, 0))
        meta["skills_in_scope"] = max(
            int(meta["skills_in_scope"]),
            int(report.get("meta", {}).get("skills_in_scope", 0)),
        )

    return {
        "skill_reports": sorted(by_skill.values(), key=lambda x: x.get("skill_name", "")),
        "skills_never_fired": sorted(never_fired.values(), key=lambda x: x.get("skill_name", "")),
        "competition_pairs": sorted(
            competition_pairs.values(), key=lambda x: (x.get("skill_a", ""), x.get("skill_b", ""))
        ),
        "coverage_gaps": sorted(coverage_gaps.values(), key=lambda x: x.get("unmet_intent", "")),
        "meta": meta,
    }


def merge_skill_report(by_skill: dict[str, dict], incoming: dict) -> None:
    name = incoming.get("skill_name", "")
    current = by_skill.setdefault(
        name,
        {
            "skill_name": name,
            "skill_path": incoming.get("skill_path", ""),
            "description_excerpt": incoming.get("description_excerpt", ""),
            "stats": {
                "total_fires": 0,
                "correct_fires": 0,
                "false_positives": 0,
                "false_negatives": 0,
                "accuracy": 0.0,
            },
            "incidents": [],
            "health_assessment": incoming.get("health_assessment", ""),
            "suggested_fix": incoming.get("suggested_fix"),
        },
    )
    for key in ("total_fires", "correct_fires", "false_positives", "false_negatives"):
        current["stats"][key] += int(incoming.get("stats", {}).get(key, 0))
    total = current["stats"]["total_fires"]
    current["stats"]["accuracy"] = current["stats"]["correct_fires"] / total if total else 0.0
    current["incidents"].extend(incoming.get("incidents", []))
    if incoming.get("suggested_fix"):
        current["suggested_fix"] = incoming["suggested_fix"]
        current["health_assessment"] = incoming.get("health_assessment", current["health_assessment"])


def merge_competition_pair(pairs: dict[tuple[str, str], dict], key: tuple[str, str], incoming: dict) -> None:
    current = pairs.setdefault(key, dict(incoming))
    if current is incoming:
        return
    current["incidents"] = int(current.get("incidents", 0)) + int(incoming.get("incidents", 0))
    if incoming.get("boundary_suggestion") and incoming["boundary_suggestion"] not in current.get(
        "boundary_suggestion", ""
    ):
        current["boundary_suggestion"] = (
            f"{current.get('boundary_suggestion', '')} {incoming['boundary_suggestion']}".strip()
        )


def merge_coverage_gap(gaps: dict[str, dict], incoming: dict) -> None:
    intent = incoming.get("unmet_intent", "")
    current = gaps.setdefault(intent, dict(incoming))
    if current is incoming:
        return
    current["frequency"] = int(current.get("frequency", 0)) + int(incoming.get("frequency", 0))
    sessions = set(current.get("related_sessions", []))
    sessions.update(incoming.get("related_sessions", []))
    current["related_sessions"] = sorted(sessions)


def update_health_history(base_dir: Path, workspace: Path) -> Path:
    history_path = base_dir / "health_history.json"
    history = read_json_list(history_path) if history_path.is_file() else []

    audit = read_json_dict(workspace / "audit_report.json")
    portfolio = read_json_dict(workspace / "portfolio_analysis.json")
    manifest = read_json_dict(workspace / "skill_manifest.json")
    proposals_path = workspace / "improvement_proposals.json"
    proposals = read_json_dict(proposals_path) if proposals_path.is_file() else {}

    health = portfolio.get("portfolio_health", {})
    entry = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "sessions_analyzed": audit.get("meta", {}).get("sessions_analyzed", 0),
        "turns_analyzed": audit.get("meta", {}).get("turns_analyzed", 0),
        "portfolio_health": health.get("overall_score", "unknown"),
        "routing_accuracy_avg": health.get("routing_accuracy_avg", 0.0),
        "total_description_tokens": manifest.get("attention_budget", {}).get("total_description_tokens", 0),
        "competition_conflicts": health.get("competition_conflicts", 0),
        "coverage_gaps": health.get("coverage_gaps", 0),
        "skills_audited": len(audit.get("skill_reports", [])),
        "patches_proposed": len(proposals.get("patches", [])),
    }
    history.append(entry)
    write_json(history_path, history)
    write_json(workspace / "health_history.json", history)
    return history_path


def generate_workspace_report(workspace: Path) -> Path:
    html = generate_report.generate_report(str(workspace))
    output = workspace / "skill_audit_report.html"
    output.write_text(html, encoding="utf-8")
    return output


def write_json(path: Path, data: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(data, indent=2, ensure_ascii=False, default=str) + "\n",
        encoding="utf-8",
    )


def read_json(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def read_json_dict(path: Path) -> dict:
    data = read_json(path)
    if not isinstance(data, dict):
        raise TypeError(f"Expected JSON object: {path}")
    return data


def read_json_list(path: Path) -> list:
    data = read_json(path)
    if not isinstance(data, list):
        raise TypeError(f"Expected JSON array: {path}")
    return data


def print_collection_summary(transcripts: dict, manifest: dict, batches: list[dict], workspace: Path) -> None:
    transcript_summary = transcripts["summary"]
    skill_summary = manifest["summary"]
    budget = manifest["attention_budget"]
    print(
        f"Collected {transcript_summary['total_sessions']} sessions, "
        f"{transcript_summary['total_user_turns']} user turns, "
        f"{skill_summary['total_skills']} skills. "
        f"Attention budget: {budget['total_description_tokens']} tokens."
    )
    print(f"Prepared {len(batches)} routing batches.")
    print(f"Workspace: {workspace}")
    print(f"Next: run prompts in {workspace / 'agent-prompts'}")


def open_report(path: Path) -> None:
    subprocess.run(["open", str(path)], check=False)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Coordinate skill-auditor collection, prompts, merge, and report generation"
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare = subparsers.add_parser("prepare", help="collect data and write sub-agent prompts")
    prepare.add_argument("--provider", choices=("claude", "codex"), default="claude")
    prepare.add_argument("project_path", nargs="?", default=None)
    prepare.add_argument("--cwd", default=None)
    prepare.add_argument("--days", type=int, default=DEFAULT_DAYS)
    prepare.add_argument("--min-turns", type=int, default=1)
    prepare.add_argument("--language", default="Japanese")
    prepare.add_argument("--base-dir", type=Path, default=None)
    prepare.add_argument("--workspace", type=Path, default=None)
    prepare.add_argument("--batch-size", type=int, default=DEFAULT_BATCH_SIZE)
    prepare.add_argument("--max-batches", type=int, default=DEFAULT_MAX_BATCHES)

    merge = subparsers.add_parser("merge", help="merge batch_audit_*.json into audit_report.json")
    merge.add_argument("--workspace", type=Path, required=True)

    report = subparsers.add_parser("report", help="update history and generate HTML report")
    report.add_argument("--workspace", type=Path, required=True)
    report.add_argument("--base-dir", type=Path, default=None)
    report.add_argument("--open", action="store_true")

    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.command == "prepare":
        project_path = args.project_path
        if project_path is None and args.cwd:
            project_path = collect_transcripts.auto_detect_project(
                args.cwd,
                provider=args.provider,
            )
        if project_path is None:
            project_path = "all"
        config = RunConfig(
            provider=args.provider,
            project_path=project_path,
            days=args.days,
            min_turns=args.min_turns,
            language=args.language,
            base_dir=args.base_dir,
            workspace=args.workspace,
            batch_size=args.batch_size,
            max_batches=args.max_batches,
        )
        prepare_run(config)
        return

    if args.command == "merge":
        output = merge_workspace(args.workspace)
        print(f"Merged audit report: {output}")
        return

    if args.command == "report":
        base_dir = args.base_dir or args.workspace.parent
        if (args.workspace / "portfolio_analysis.json").is_file():
            update_health_history(base_dir, args.workspace)
        output = generate_workspace_report(args.workspace)
        print(f"Generated report: {output}")
        if args.open:
            open_report(output)
        return

    raise AssertionError(f"Unhandled command: {args.command}")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)
