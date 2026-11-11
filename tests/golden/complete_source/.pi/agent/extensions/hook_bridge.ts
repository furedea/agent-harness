/**
 * Bridges pi extension events to agent-harness hooks.
 *
 * Forwards each event to ~/.pi/hooks/hook_adapter.py, which matches it
 * against ~/.pi/agent/hooks.json and runs the configured commands. The
 * adapter's blocking output is translated back into pi result shapes.
 */
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

const ADAPTER =
	process.env.AGENT_HARNESS_PI_ADAPTER ?? join(homedir(), ".pi", "hooks", "hook_adapter.py");
const TIMEOUT_MS = Number(process.env.AGENT_HARNESS_HOOK_TIMEOUT ?? "30") * 1000;

interface AdapterRun {
	code: number;
	stdout: string;
	stderr: string;
}

function dispatch(event: string, payload: unknown): Promise<AdapterRun> {
	return new Promise((resolve) => {
		const run: AdapterRun = { code: -1, stdout: "", stderr: "" };
		const done = (result: AdapterRun) => {
			clearTimeout(timer);
			resolve(result);
		};
		// -I -B mirrors the adapter shebang, which a bare interpreter call would bypass.
		const child = spawn("python3", ["-I", "-B", ADAPTER, "dispatch", event], {
			stdio: ["pipe", "pipe", "pipe"],
		});
		const timer = setTimeout(() => {
			child.kill("SIGKILL");
			done(run);
		}, TIMEOUT_MS);
		child.stdout.on("data", (data) => (run.stdout += data));
		child.stderr.on("data", (data) => (run.stderr += data));
		child.on("error", () => done(run));
		child.on("close", (code) => done({ ...run, code: code ?? -1 }));
		child.stdin.end(JSON.stringify(payload));
	});
}

type HookDecision = { kind: "deny" | "ask"; reason: string };

function hookDecision(run: AdapterRun): HookDecision | undefined {
	let ask: HookDecision | undefined;
	if (run.code === 2) return { kind: "deny", reason: run.stderr.trim() || "blocked by agent-harness hook" };
	const lines = run.stdout.split("\n");
	for (let i = lines.length - 1; i >= 0; i--) {
		const line = lines[i].trim();
		if (!line.startsWith("{")) continue;
		try {
			const data: unknown = JSON.parse(line);
			if (!isRecord(data)) continue;
			if (data?.decision === "block") return { kind: "deny", reason: String(data.reason || "blocked by agent-harness hook") };
			if (data?.action === "block") return { kind: "deny", reason: String(data.message || "blocked by agent-harness hook") };
			const output = data?.hookSpecificOutput;
			if (isRecord(output) && output.permissionDecision === "deny") return { kind: "deny", reason: String(output.permissionDecisionReason || "blocked by agent-harness hook") };
			if (isRecord(output) && output.permissionDecision === "ask") {
				ask = { kind: "ask", reason: String(output.permissionDecisionReason || "Confirmation required by agent-harness hook") };
			}
		} catch {
			continue;
		}
	}
	return ask;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null;
}

function sessionId(ctx: ExtensionContext): string {
	try {
		return ctx.sessionManager.getSessionId() ?? "";
	} catch {
		return "";
	}
}

type RawEvent = Record<string, unknown>;

export default function (pi: ExtensionAPI) {
	if (!existsSync(ADAPTER)) return;

	const forward = (event: string, rawEvent: unknown, ctx: ExtensionContext, extra?: RawEvent) => {
		const base = typeof rawEvent === "object" && rawEvent !== null ? (rawEvent as RawEvent) : {};
		return dispatch(event, {
			hook_event_name: event,
			cwd: ctx.cwd,
			session_id: sessionId(ctx),
			...base,
			...extra,
		});
	};

	const observe = (event: string) => {
		return async (rawEvent: unknown, ctx: ExtensionContext) => {
			await forward(event, rawEvent, ctx);
			return undefined;
		};
	};

	for (const event of [
		"session_start",
		"session_shutdown",
		"session_before_compact",
		"session_compact",
		"before_agent_start",
		"agent_start",
		"agent_end",
		"agent_settled",
		"turn_start",
		"turn_end",
		"tool_result",
		"user_bash",
	]) {
		pi.on(event, observe(event));
	}

	pi.on("tool_call", async (event, ctx) => {
		const run = await forward("tool_call", event, ctx, {
			tool_name: event.toolName,
			tool_input: event.input,
		});
		const decision = hookDecision(run);
		if (decision === undefined) return undefined;
		const { reason } = decision;
		if (decision.kind === "deny") return { block: true, reason };
		if (!ctx.hasUI) return { block: true, reason: reason + " (no UI to confirm; blocked)" };
		const preview = JSON.stringify(event.input);
		const answer = await ctx.ui.select(reason + "\n" + event.toolName + ": " + preview, ["Yes", "No"]);
		if (answer !== "Yes") return { block: true, reason: "Blocked by user: " + reason };
		return undefined;
	});

	pi.on("input", async (event, ctx) => {
		const run = await forward("input", event, ctx, { text: event.text });
		const decision = hookDecision(run);
		if (decision?.kind !== "deny") return { action: "continue" };
		if (ctx.hasUI) ctx.ui.notify(decision.reason, "warning");
		return { action: "handled" };
	});
}
