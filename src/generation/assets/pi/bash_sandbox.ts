/**
 * Replaces every built-in agent filesystem tool. Pi and its hooks stay outside srt.
 * Requires srt with recursive denyGlobRegex support (not nixpkgs 0.0.52).
 */
import { execFileSync, spawn } from "node:child_process";
import { lstatSync, mkdtempSync, readFileSync, realpathSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { dirname, join, resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { createBashTool, createReadTool, createWriteTool, createEditTool, createGrepTool, createFindTool, createLsTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";

type Policy = {
	network: { allowedDomains: string[]; deniedDomains: string[]; allowUnixSockets: string[] };
	filesystem: { denyRead: string[]; allowWrite: string[]; denyWrite: string[] };
};

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isStringArray(value: unknown): value is string[] {
	return Array.isArray(value) && value.every(item => typeof item === "string" && item.length > 0 && !item.includes("\0"));
}

function isDomainPattern(value: string, deny: boolean): boolean {
	const match = /^(\*|\[[0-9a-fA-F:.]+\]|(?:\*\.)?[a-zA-Z0-9-]+(?:\.[a-zA-Z0-9-]+)*)(?::([0-9]+))?$/.exec(value);
	if (!match) return false;
	const host = match[1];
	if (host === "*") {
		if (!deny) return false;
	} else if (!host.startsWith("[") && host !== "localhost" && !host.replace(/^\*\./, "").includes(".")) {
		return false;
	}
	return !match[2] || (/^[1-9][0-9]{0,4}$/.test(match[2]) && Number(match[2]) <= 65535);
}

// Accept only the generated subset of srt's schema; never pass unknown options to srt.
// In particular, incomplete configs make some srt versions silently use defaults.
function validatePolicy(value: unknown): asserts value is Policy {
	const fail = (reason: string): never => { throw new Error("Failed to validate srt sandbox policy: " + reason); };
	if (!isRecord(value)) return fail("the policy is not an object");
	const unknownTop = Object.keys(value).find(key => !["network", "filesystem"].includes(key));
	if (unknownTop !== undefined) return fail("unknown field " + JSON.stringify(unknownTop));
	const { network, filesystem } = value;
	if (!isRecord(network)) return fail("network is not an object");
	if (!isRecord(filesystem)) return fail("filesystem is not an object");
	const sections = [
		["network", network, ["allowedDomains", "deniedDomains", "allowUnixSockets"]],
		["filesystem", filesystem, ["denyRead", "allowWrite", "denyWrite"]],
	] as const;
	for (const [section, object, keys] of sections) {
		const unknown = Object.keys(object).find(key => !(keys as readonly string[]).includes(key));
		if (unknown !== undefined) return fail("unknown field " + JSON.stringify(section + "." + unknown));
		for (const key of keys) {
			if (!isStringArray(object[key])) return fail(section + "." + key + " is not an array of nonempty strings");
		}
	}
	const badDomain = (network.allowedDomains as string[]).find(domain => !isDomainPattern(domain, false));
	if (badDomain !== undefined) return fail("network.allowedDomains has an invalid domain " + JSON.stringify(badDomain));
	const badDenied = (network.deniedDomains as string[]).find(domain => !isDomainPattern(domain, true));
	if (badDenied !== undefined) return fail("network.deniedDomains has an invalid domain " + JSON.stringify(badDenied));
}

function canonical(path: string): string {
	try {
		return realpathSync(path);
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
		return join(canonical(dirname(path)), basename(path));
	}
}

function isSymlink(path: string): boolean {
	try {
		return lstatSync(path).isSymbolicLink();
	} catch {
		return false;
	}
}

// A protected symlink must keep its own rule as well as its target's: the link itself can be replaced.
function protectedPaths(path: string, base: string): string[] {
	const expanded = path.replace(/^(~|\$HOME)(?=\/|$)/, homedir());
	const absolute = resolve(base, expanded);
	const literal = policyPath(dirname(absolute), base) + "/" + basename(absolute);
	// Intermediate components that are symlinks (e.g. a linked ~/.claude) can be replaced too.
	const links: string[] = [];
	for (let part = absolute; part !== dirname(part); part = dirname(part)) {
		if (isSymlink(part) && part !== absolute) links.push(policyPath(dirname(part), base) + "/" + basename(part));
	}
	return [...new Set([literal, policyPath(path, base), ...links])];
}

function policyPath(path: string, base: string): string {
	const expanded = path.replace(/^(~|\$HOME)(?=\/|$)/, homedir());
	const absolute = resolve(base, expanded);
	// Resolve the literal ancestor of a glob without expanding its pattern.
	const glob = absolute.search(/[*?\[\]{]/);
	if (glob < 0) return canonical(absolute);
	const slash = absolute.lastIndexOf("/", glob);
	return join(canonical(absolute.slice(0, slash) || "/"), absolute.slice(slash + 1));
}

function quote(value: string): string {
	return "'" + value.replaceAll("'", "'\\''") + "'";
}

type Prepared = { settingsPath: string; content: string; common: string | undefined; root: string };

// Variables that select a repository instead of letting git discover it from the cwd. Pi may be started
// by a Git hook, and an inherited one would point discovery at another repository.
const GIT_SELECTION_VARIABLES = ["GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_NAMESPACE", "GIT_PREFIX"];

function gitDiscoveryEnv(): NodeJS.ProcessEnv {
	const env = { ...process.env };
	for (const name of GIT_SELECTION_VARIABLES) delete env[name];
	return env;
}

// Any failure (no repository, no git, unusable layout) reads as "no repository": the caller then denies .git.
function gitCommonDir(cwd: string): string | undefined {
	try {
		const out = execFileSync("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd, env: gitDiscoveryEnv(), encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
		// Drop only git's terminating newline: a directory name may end in whitespace.
		return canonical(out.replace(/\n$/, ""));
	} catch {
		return undefined;
	}
}

// Relative secret entries are anchored here, as gitignore and Codex anchor them. Outside a repository
// (or when git fails) the working directory is the root.
function gitRoot(cwd: string): string {
	try {
		const out = execFileSync("git", ["rev-parse", "--show-toplevel"], { cwd, env: gitDiscoveryEnv(), encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
		// Drop only git's terminating newline: a directory name may end in whitespace.
		return canonical(out.replace(/\n$/, ""));
	} catch {
		return cwd;
	}
}

// Only the repository's own common dir is left to the common-dir rules; a pointer file, an absent .git
// or a directory that is not the common dir could be planted to repoint git.
function isCommonDirectory(path: string, common: string): boolean {
	try {
		return lstatSync(path).isDirectory() && canonical(path) === common;
	} catch {
		return false;
	}
}

// Files git reads to find its config and hooks, in the common dir and in every git dir below it
// (linked worktrees, submodules, and submodules of linked worktrees). One list, so scopes cannot drift.
const GIT_CONTROL_FILES = ["hooks", "config", "config.worktree", "commondir", "gitdir"];

function gitControlPaths(common: string): string[] {
	const scopes = ["", "worktrees/*/", "modules/**/", "worktrees/*/modules/**/"];
	return scopes.flatMap(scope => GIT_CONTROL_FILES.map(file => join(common, scope, file)));
}

// Policy for one worktree: the static source policy plus what only the runtime knows.
function buildPolicy(source: Policy, cwd: string, root: string, temporary: string, protectedFiles: string[], common: string | undefined): Policy {
	const policy: Policy = structuredClone(source);
	policy.filesystem.allowWrite = policy.filesystem.allowWrite.map(path => policyPath(path, cwd));
	// Deny-only, so a relative entry is resolved at the root and at the cwd: a source entry that relied on the cwd anchor keeps its reach.
	policy.filesystem.denyRead = [...new Set(policy.filesystem.denyRead.flatMap(path => [...protectedPaths(path, root), ...protectedPaths(path, cwd)]))];
	policy.filesystem.denyWrite = policy.filesystem.denyWrite.flatMap(path => protectedPaths(path, cwd));
	policy.network.allowUnixSockets = policy.network.allowUnixSockets.map(path => policyPath(path, cwd));
	policy.filesystem.allowWrite.push(cwd);
	policy.filesystem.denyWrite.push(temporary, ...protectedFiles);
	if (common) {
		policy.filesystem.allowWrite.push(common);
		policy.filesystem.denyWrite.push(...gitControlPaths(common));
	}
	// Pi loads project-local extensions and settings from here, unsandboxed, on its next start.
	policy.filesystem.denyWrite.push(join(cwd, ".pi"));
	// Nested repositories and submodule gitfiles; "*/" keeps the worktree's own .git out of this pattern.
	policy.filesystem.denyWrite.push(join(cwd, "*/**/.git"));
	// git runs outside the sandbox, so a .git the sandbox can create or rewrite would make it honour attacker config.
	if (!common || !isCommonDirectory(join(cwd, ".git"), common)) policy.filesystem.denyWrite.push(join(cwd, ".git"));
	validatePolicy(policy);
	return policy;
}

export default function (pi: ExtensionAPI) {
	const startCwd = realpathSync(process.cwd());
	const prepared = new Map<string, Prepared>();
	let source: Policy | undefined;
	let failure: unknown;
	let temporary: string | undefined;
	let protectedFiles: string[] = [];
	try {
		const sourcePath = fileURLToPath(new URL("./sandbox_policy.json", import.meta.url));
		const parsed: unknown = JSON.parse(readFileSync(sourcePath, "utf8"));
		validatePolicy(parsed);
		source = parsed;
		temporary = realpathSync(mkdtempSync(join(tmpdir(), "pi-srt-")));
		protectedFiles = [canonical(sourcePath), fileURLToPath(import.meta.url), fileURLToPath(new URL("./sandbox/file_operations.mjs", import.meta.url))];
	} catch (error) {
		failure = error;
	}

	// Rebuilt on every call because the repository layout can change between commands.
	function prepare(cwdInput: string): Prepared {
		if (failure || !source || !temporary) throw new Error("Sandbox unavailable: " + String(failure));
		const cwd = realpathSync(cwdInput);
		const previous = prepared.get(cwd);
		if (previous && readFileSync(previous.settingsPath, "utf8") !== previous.content) throw new Error("Sandbox settings changed");
		// The sandbox can rewrite .git pointers, so a cwd's first resolution (repository or not) is final for the session.
		const common = gitCommonDir(cwd);
		if (previous && previous.common !== common) throw new Error("Sandbox git layout changed");
		// The root comes from the same pointers, so it is pinned with them.
		const root = gitRoot(cwd);
		if (previous && previous.root !== root) throw new Error("Sandbox git layout changed");
		const content = JSON.stringify(buildPolicy(source, cwd, root, temporary, protectedFiles, common));
		const settingsPath = previous?.settingsPath ?? join(temporary, prepared.size + ".json");
		if (content !== previous?.content) writeFileSync(settingsPath, content, { mode: 0o600 });
		const entry = { settingsPath, content, common, root };
		prepared.set(cwd, entry);
		return entry;
	}

	function toolFor(cwd: string) {
		return createBashTool(cwd, {
			spawnHook: ({ command, cwd, env }) => {
				const { settingsPath } = prepare(cwd);
				return { command: "exec srt --settings " + quote(settingsPath) + " -c " + quote(command), cwd, env };
			},
		});
	}

	async function request<T>(cwd: string, operation: Record<string, unknown>, signal?: AbortSignal): Promise<T> {
		const description = "Sandbox " + String(operation.tool ?? operation.operation) + " for " + String(operation.path ?? cwd);
		const { settingsPath } = prepare(cwd);
		const helper = fileURLToPath(new URL("./sandbox/file_operations.mjs", import.meta.url));
		const command = [process.execPath, helper, import.meta.resolve("@earendil-works/pi-coding-agent")].map(quote).join(" ");
		return new Promise((resolve, reject) => {
			const fail = (error: unknown) => reject(Object.assign(
				new Error(description + " failed: " + String(error), { cause: error }),
				{ code: (error as NodeJS.ErrnoException)?.code },
			));
			if (signal?.aborted) return reject(new Error(description + " aborted"));
			const child = spawn("srt", ["--settings", settingsPath, "-c", command], { cwd, stdio: ["pipe", "pipe", "pipe"], detached: true });
			let output = "";
			let stderr = "";
			const abort = () => { if (child.pid) { try { process.kill(-child.pid, "SIGKILL"); } catch {} } };
			signal?.addEventListener("abort", abort, { once: true });
			child.stdout.on("data", chunk => { output += chunk; });
			child.stderr.on("data", chunk => { stderr += chunk; });
			child.on("error", fail);
			child.stdin.on("error", fail);
			child.on("close", code => {
				signal?.removeEventListener("abort", abort);
				try {
					if (signal?.aborted) return reject(new Error(description + " aborted"));
					const result = JSON.parse(output);
					if (result.error) return reject(Object.assign(new Error(description + " failed: " + result.error.message), { code: result.error.code }));
					if (code !== 0) return reject(new Error(description + " helper failed: " + stderr));
					resolve(result.value);
				} catch (error) { fail(error); }
			});
			child.stdin.end(JSON.stringify(operation));
		});
	}

	function readFor(cwd: string, signal?: AbortSignal) {
		return createReadTool(cwd, { operations: {
			readFile: async path => Buffer.from(await request<string>(cwd, { operation: "readFile", path }, signal), "base64"),
			access: path => request<void>(cwd, { operation: "access", path }, signal),
			detectImageMimeType: path => request<string | null>(cwd, { operation: "detectImageMimeType", path }, signal),
		} });
	}
	function writeFor(cwd: string, signal?: AbortSignal) {
		return createWriteTool(cwd, { operations: {
			writeFile: (path, content) => request<void>(cwd, { operation: "writeFile", path, content }, signal),
			mkdir: path => request<void>(cwd, { operation: "mkdir", path }, signal),
		} });
	}
	function editFor(cwd: string, signal?: AbortSignal) {
		return createEditTool(cwd, { operations: {
			readFile: async path => Buffer.from(await request<string>(cwd, { operation: "readFile", path }, signal), "base64"),
			access: path => request<void>(cwd, { operation: "access", path, write: true }, signal),
			writeFile: (path, content) => request<void>(cwd, { operation: "writeFile", path, content }, signal),
		} });
	}
	pi.registerTool({
		...toolFor(startCwd),
		execute: async (id, params, signal, onUpdate, ctx) =>
			toolFor(ctx?.cwd ?? startCwd).execute(id, params, signal, onUpdate),
	});
	pi.registerTool({
		...readFor(startCwd),
		execute: async (id, params, signal, onUpdate, ctx) =>
			readFor(ctx?.cwd ?? startCwd, signal).execute(id, params, signal, onUpdate),
	});
	pi.registerTool({
		...writeFor(startCwd),
		execute: async (id, params, signal, onUpdate, ctx) =>
			writeFor(ctx?.cwd ?? startCwd, signal).execute(id, params, signal, onUpdate),
	});
	pi.registerTool({
		...editFor(startCwd),
		execute: async (id, params, signal, onUpdate, ctx) =>
			editFor(ctx?.cwd ?? startCwd, signal).execute(id, params, signal, onUpdate),
	});
	// grep spawns rg outside its operations interface, so run the entire search tool in srt.
	for (const factory of [createGrepTool, createFindTool, createLsTool]) {
		const tool = factory(startCwd);
		pi.registerTool({ ...tool, execute: async (id, params, signal, _onUpdate, ctx) =>
			request(ctx?.cwd ?? startCwd, { operation: "search", tool: tool.name, id, params }, signal) });
	}
	const allowed = new Set(["bash", "read", "write", "edit", "grep", "find", "ls"]);
	pi.on("before_agent_start", async () => {
		// Narrow only: never switch on a tool the user left off.
		pi.setActiveTools(pi.getActiveTools().filter(name => allowed.has(name)));
	});
	pi.on("tool_call", async event => {
		if (!allowed.has(event.toolName)) {
			return { block: true, reason: "Sandbox requires only the managed built-in tools; extension and MCP tools are unsupported" };
		}
	});
	pi.on("session_shutdown", async () => {
		if (temporary) rmSync(temporary, { recursive: true, force: true });
	});
}
