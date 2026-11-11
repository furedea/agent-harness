/**
 * Bridges pi extension events to agent-harness hooks.
 *
 * Forwards each event to ~/.pi/hooks/hook_adapter.py, which matches it
 * against ~/.pi/agent/hooks.json and runs the configured commands. The
 * adapter's blocking output is translated back into pi result shapes.
 */
import { spawn } from "node:child_process";
import { existsSync, readFileSync, realpathSync, lstatSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";
import { join, resolve, dirname, basename, relative, isAbsolute, sep } from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

const ADAPTER =
	process.env.AGENT_HARNESS_PI_ADAPTER ?? join(homedir(), ".pi", "hooks", "hook_adapter.py");
const SANDBOX_POLICY =
	process.env.AGENT_HARNESS_PI_SANDBOX_POLICY ?? join(dirname(fileURLToPath(import.meta.url)), "sandbox_policy.json");
const SEARCH_TOOLS = ["grep", "find", "ls"];
const FILE_TOOLS = ["read", "write", "edit", ...SEARCH_TOOLS];
const TIMEOUT_MS = Number(process.env.AGENT_HARNESS_HOOK_TIMEOUT ?? "30") * 1000;

interface AdapterRun {
	code: number;
	stdout: string;
	stderr: string;
}

function dispatch(event: string, payload: unknown): Promise<AdapterRun> {
	return new Promise((resolve) => {
		const run: AdapterRun = { code: -1, stdout: "", stderr: "" };
		// A missing adapter must not read as exit 2 (deny) from the interpreter's "can't open file".
		if (!existsSync(ADAPTER)) return resolve(run);
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
		child.stdin.on("error", () => done(run));
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

// Resolve existing ancestors as well as targets that have not been created yet.
function targetPath(path: string): string {
	try {
		return realpathSync(path);
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
		// A dangling symlink must never become an apparently safe missing file.
		try {
			if (lstatSync(path).isSymbolicLink()) throw new Error("Cannot resolve file target " + path + ": dangling symlink");
		} catch (statError) {
			if ((statError as NodeJS.ErrnoException).code !== "ENOENT") throw statError;
		}
		return join(targetPath(dirname(path)), basename(path));
	}
}

// Mirror how pi resolves a tool path so the check sees the file pi will open.
function normalizeToolPath(path: string): string {
	const plain = path.replace(/[\u00a0\u2000-\u200a\u202f\u205f\u3000]/g, " ").replace(/^@/, "");
	if (plain === "~") return homedir();
	return plain.startsWith("~/") ? join(homedir(), plain.slice(2)) : plain;
}

// Case-insensitive, ignoring format characters (zero-width etc.), stream suffixes, trailing dots/spaces
// and 8.3 aliases that case-folding or NTFS/HFS+ filesystems map onto .git.
function touchesGit(path: string): boolean {
	return path.split(sep).some(part => {
		const name = part.normalize("NFC").replace(/\p{Cf}/gu, "").split(":")[0];
		return /^(\.git|git~\d+)[. ]*$/i.test(name);
	});
}

// Mirrors the srt denyRead entry for Pi's own credentials, which the file tools would otherwise bypass.
function isCredentialFile(path: string): boolean {
	// HOME may not exist (build sandboxes); then only the lexical form can match.
	let realHome = homedir();
	try {
		realHome = realpathSync(realHome);
	} catch {}
	// Case-folded and NFC-normalized because macOS volumes are case-insensitive by default.
	const fold = (value: string) => value.normalize("NFC").toLowerCase();
	const candidates = [homedir(), realHome].map(home => fold(join(home, ".pi", "agent", "auth.json")));
	if (candidates.includes(fold(path))) return true;
	return [homedir(), realHome].some(home => sameFile(path, join(home, ".pi", "agent", "auth.json")));
}

// Hard links make the same file reachable under another name.
function sameFile(a: string, b: string): boolean {
	try {
		const left = statSync(a);
		const right = statSync(b);
		return left.dev === right.dev && left.ino === right.ino;
	} catch {
		return false;
	}
}

// srt only wraps bash, so the generated deny rules are also enforced on the file tools.
interface DenyRule {
	pattern: RegExp;
	literal?: string;
}

function denyRule(entry: string, root: string): DenyRule {
	// Anything the translator cannot honour must block rather than silently not match.
	if (/[\[\]{}\0]/.test(entry)) throw new Error("Unsupported glob syntax: " + entry);
	const lexical = resolve(root, normalizeToolPath(entry.replace(/^\$HOME(?=\/|$)/, homedir())));
	// Resolve symlinks (macOS /var) in the literal part only, as targets are compared in resolved form.
	const glob = lexical.search(/[*?]/);
	const split = glob < 0 ? lexical.length : lexical.lastIndexOf("/", glob);
	const absolute = join(targetPath(lexical.slice(0, split) || "/"), lexical.slice(split));
	const body = absolute
		.split(/(\*\*\/|\*\*|\*|\?)/)
		.map(part => ({ "**/": "(?:.*/)?", "**": ".*", "*": "[^/]*", "?": "[^/]" })[part] ?? part.replace(/[.+^${}()|[\]\\]/g, "\\$&"))
		.join("");
	// Case-insensitive because default macOS volumes are; on case-sensitive ones this only denies more.
	return { pattern: new RegExp("^" + body.normalize("NFC") + "(?:/.*)?$", "i"), literal: glob < 0 ? absolute : undefined };
}

// A missing or malformed policy blocks the file tools, as it does bash.
function denyRules(root: string): { read: DenyRule[]; write: DenyRule[] } | undefined {
	if (!existsSync(SANDBOX_POLICY)) return undefined;
	const policy: unknown = JSON.parse(readFileSync(SANDBOX_POLICY, "utf8"));
	if (!isRecord(policy) || !isRecord(policy.filesystem)) return undefined;
	const { denyRead, denyWrite } = policy.filesystem;
	if (!Array.isArray(denyRead) || !Array.isArray(denyWrite)) return undefined;
	if (![...denyRead, ...denyWrite].every(entry => typeof entry === "string" && entry)) return undefined;
	return { read: denyRead.map(entry => denyRule(entry, root)), write: denyWrite.map(entry => denyRule(entry, root)) };
}

// The path and every ancestor, so a literal directory rule matches its descendants by identity too.
function ancestry(path: string): string[] {
	const entries = [path];
	for (let parent = dirname(path); parent !== entries[entries.length - 1]; parent = dirname(parent)) entries.push(parent);
	return entries;
}

function isDenied(rules: DenyRule[], paths: string[]): boolean {
	return rules.some(rule => paths.some(path => rule.pattern.test(path.normalize("NFC")) || (rule.literal !== undefined && ancestry(path).some(entry => sameFile(entry, rule.literal!)))));
}

function fileBoundary(tool: string, input: unknown, cwd: string): string | undefined {
	if (!isRecord(input)) return "Invalid file path";
	// grep, find and ls default to the working directory when no path is given.
	const given = input.path ?? (SEARCH_TOOLS.includes(tool) ? "." : undefined);
	if (typeof given !== "string" || !given) return "Invalid file path";
	try {
		const isWrite = tool === "write" || tool === "edit";
		const root = realpathSync(cwd);
		const lexical = resolve(root, normalizeToolPath(given));
		const target = targetPath(lexical);
		if ([lexical, target].some(touchesGit)) return "File tools cannot access .git";
		const rules = denyRules(root);
		if (!rules) return "Invalid sandbox policy";
		const denied = !isWrite ? rules.read : [...rules.read, ...rules.write];
		if (isDenied(denied, [lexical, target])) return "Path is denied by the sandbox policy";
		if ([lexical, target].some(isCredentialFile)) return "File tools cannot access Pi credentials";
		const path = relative(root, target);
		if (isWrite && (path === ".." || path.startsWith(".." + sep) || isAbsolute(path))) {
			return "File writes must stay inside the worktree";
		}
	} catch {
		return "Cannot resolve file target safely";
	}
	return undefined;
}

export default function (pi: ExtensionAPI) {

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
		const fileTool = FILE_TOOLS.includes(event.toolName);
		if (fileTool) {
			const reason = fileBoundary(event.toolName, event.input, ctx.cwd);
			if (reason) return { block: true, reason };
		}
		const run = await forward("tool_call", event, ctx, {
			tool_name: event.toolName,
			tool_input: event.input,
		});
		if (fileTool && run.code !== 0 && run.code !== 2) {
			return { block: true, reason: "File tool hook failed" };
		}
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
