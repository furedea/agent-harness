# Architecture Reference

Design decisions and execution architecture of the Skill Auditor.

---

## Execution Architecture: Sub-Agent Model

The analysis uses host sub-agents: Claude Code's Agent tool or Codex's `spawn_agent`.

### Why Sub-Agents

| Approach | Pros | Cons |
| --- | --- | --- |
| Inline (coordinator does analysis) | Simple | Context compaction destroys data mid-analysis |
| External API (Gemini etc.) | Large context | Requires API key, additional cost, vendor dependency |
| **Sub-agent** (chosen) | No external deps, host agent analyzes own behavior | Need careful batching |

Sub-agents are the right fit because:

- **No external dependency**: No API keys, no additional runtime service
- **Self-analysis advantage**: The host agent analyzing its own skill routing behavior has natural domain understanding
- **Progressive disclosure**: Each sub-agent reads only its batch of data, keeping context focused
- **Parallelism**: Multiple routing-analyst sub-agents can run simultaneously on different batches

### Batching Strategy

When transcripts exceed what a single sub-agent can analyze effectively:

1. Sort sessions by timestamp
2. Split into batches (~30 sessions per batch as a guideline)
3. Each routing-analyst sub-agent gets: full skill manifest + one batch
4. Coordinator merges results: union of incidents, recalculate per-skill stats

The coordinator manages batching, not the sub-agents. Sub-agents receive their data and produce their analysis.

---

## Orchestration Flow

```
SKILL.md (Coordinator)
  |
  |-- Step 1-3: Data collection (scripts)
  |     collect_transcripts.py -> transcripts.json
  |     collect_skills.py -> skill_manifest.json
  |
  |-- Step 4: Routing Audit (sub-agents, parallel)
  |     Sub-agent(routing-analyst) x N batches -> batch_audit_N.json
  |     Coordinator merges -> audit_report.json
  |
  |-- Step 5: Portfolio Analysis (sub-agent)
  |     Sub-agent(portfolio-analyst) -> portfolio_analysis.json
  |
  |-- Step 6: Improvement Plan (sub-agent)
  |     Sub-agent(improvement_planner) -> improvement_proposals.json
  |
  |-- Step 7: HTML Report (script)
  |     generate_report.py -> skill_audit_report.html
  |     open in browser
  |
  |-- Step 8: Apply Patches (script, with user approval)
  |     apply_patches.py -> changelog.md
```

### Sub-Agent Prompt Delivery

Each sub-agent is spawned with:

1. A task description referencing the agent prompt file (e.g., agents/routing_analyst.md)
2. Instructions to read that file for detailed rubric
3. Paths to input data files
4. Path for output JSON file

The coordinator instructs the sub-agent to:

```
Read agents/routing_analyst.md for your analysis rubric.
Read <workspace>/skill_manifest.json for skill definitions.
Read <workspace>/transcripts-batch-N.json for session data.
Write your analysis to <workspace>/batch_audit_N.json.
```

---

## Workspace Structure

```
~/.<provider>/skill-report/projects/<project-slug>/
├── health_history.json           # Append-only audit history for this project slug
├── 2026-03-04T18-45-23/
│   ├── transcripts.json          # All parsed sessions
│   ├── transcripts-batch-*.json  # Per-batch session files (if batched)
│   ├── skill_manifest.json       # Skill definitions + attention budget
│   ├── batch_audit_*.json        # Per-batch routing audit results
│   ├── audit_report.json         # Merged routing audit
│   ├── portfolio_analysis.json   # Attention budget + competition matrix
│   ├── improvement_proposals.json # Patches + new skill suggestions
│   ├── patches/                  # Per-skill patch files
│   │   ├── skill-name.patch.json
│   │   └── ...
│   ├── skill_audit_report.html   # Interactive HTML report
│   └── changelog.md              # Applied changes log
└── ...
```

Cross-project mode uses `~/.<provider>/skill-report/all/`.

---

## HTML Report

The report is a self-contained HTML file with all data embedded inline (no external dependencies). Pattern follows skill-creator's eval-viewer:

1. `generate_report.py` reads all analysis JSON files
2. Embeds data into `assets/report_template.html` via placeholder replacement
3. Report sections:
    - Executive Summary (portfolio health score, key metrics)
    - Per-Skill Health Cards (accuracy, incidents, suggested fixes)
    - Competition Matrix (pair relationships, boundary suggestions)
    - Attention Budget (token distribution, trim candidates)
    - Improvement Patches (diff view, cascade risk)
    - Coverage Gaps (unmet intents, new skill proposals)
4. Opened in browser via `open` in Claude Code or Browser Use in Codex

---

## Health History

`health_history.json` is an append-only array of audit snapshots. Each run appends one entry. This enables run-over-run comparison:

```json
[
    {
        "timestamp": "2026-03-04T...",
        "sessions_analyzed": 36,
        "turns_analyzed": 316,
        "portfolio_health": "needs_attention",
        "routing_accuracy_avg": 0.82,
        "total_description_tokens": 3400,
        "competition_conflicts": 3,
        "coverage_gaps": 2,
        "skills_audited": 17
    }
]
```

The coordinator appends the current run's summary and reports the delta from the previous run (if any).
