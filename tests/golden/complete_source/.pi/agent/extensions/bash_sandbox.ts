/**
 * Replaces the agent bash tool. Pi and its hooks stay outside srt.
 * Requires srt with recursive denyGlobRegex support (not nixpkgs 0.0.52).
 */
import { execFileSync } from "node:child_process";
import { lstatSync, mkdtempSync, readFileSync, realpathSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { dirname, join, resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { createBashTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";

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
	if (!isRecord(value) || Object.keys(value).some(key => !["network", "filesystem"].includes(key))) throw new Error("Failed to validate srt sandbox policy: invalid fields");
	const network = value.network;
	const filesystem = value.filesystem;
	if (!isRecord(network) || !isRecord(filesystem) ||
		Object.keys(network).some(key => !["allowedDomains", "deniedDomains", "allowUnixSockets"].includes(key)) ||
		Object.keys(filesystem).some(key => !["denyRead", "allowWrite", "denyWrite"].includes(key)) ||
		!isStringArray(network.allowedDomains) || !isStringArray(network.deniedDomains) || !isStringArray(network.allowUnixSockets) ||
		!isStringArray(filesystem.denyRead) || !isStringArray(filesystem.allowWrite) || !isStringArray(filesystem.denyWrite) ||
		!network.allowedDomains.every(value => isDomainPattern(value, false)) ||
		!network.deniedDomains.every(value => isDomainPattern(value, true))) throw new Error("Failed to validate srt sandbox policy: invalid network or filesystem rules");
}

function canonical(path: string): string {
	try {
		return realpathSync(path);
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
		return join(canonical(dirname(path)), basename(path));
	}
}

function policyPath(path: string, cwd: string): string {
	const expanded = path.replace(/^(~|\$HOME)(?=\/|$)/, homedir());
	const absolute = resolve(cwd, expanded);
	// Resolve the literal ancestor of a glob without expanding its pattern.
	const glob = absolute.search(/[*?\[\]{]/);
	if (glob < 0) return canonical(absolute);
	const slash = absolute.lastIndexOf("/", glob);
	return join(canonical(absolute.slice(0, slash) || "/"), absolute.slice(slash + 1));
}

function quote(value: string): string {
	return "'" + value.replaceAll("'", "'\\''") + "'";
}

type Prepared = { settingsPath: string; content: string; common: string | undefined };

// Any failure (no repository, no git, unusable layout) reads as "no repository": the caller then denies .git.
function gitCommonDir(cwd: string): string | undefined {
	try {
		const out = execFileSync("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
		return canonical(out.trim());
	} catch {
		return undefined;
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

// Policy for one worktree: the static source policy plus what only the runtime knows.
function buildPolicy(source: Policy, cwd: string, temporary: string, protectedFiles: string[], common: string | undefined): Policy {
	const policy: Policy = structuredClone(source);
	for (const key of ["denyRead", "allowWrite", "denyWrite"] as const) {
		policy.filesystem[key] = policy.filesystem[key].map(path => policyPath(path, cwd));
	}
	policy.network.allowUnixSockets = policy.network.allowUnixSockets.map(path => policyPath(path, cwd));
	policy.filesystem.allowWrite.push(cwd);
	policy.filesystem.denyWrite.push(temporary, ...protectedFiles);
	if (common) {
		policy.filesystem.allowWrite.push(common);
		policy.filesystem.denyWrite.push(join(common, "hooks"), join(common, "config"), join(common, "commondir"), join(common, "config.worktree"),
			join(common, "modules/**/config"), join(common, "modules/**/hooks"),
			join(common, "worktrees/*/config.worktree"), join(common, "worktrees/*/commondir"), join(common, "worktrees/*/gitdir"),
		);
	}
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
		protectedFiles = [canonical(sourcePath), fileURLToPath(import.meta.url)];
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
		const content = JSON.stringify(buildPolicy(source, cwd, temporary, protectedFiles, common));
		const settingsPath = previous?.settingsPath ?? join(temporary, prepared.size + ".json");
		if (content !== previous?.content) writeFileSync(settingsPath, content, { mode: 0o600 });
		const entry = { settingsPath, content, common };
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

	const bash = toolFor(startCwd);
	pi.registerTool({
		...bash,
		execute: async (id, params, signal, onUpdate, ctx) => (ctx?.cwd ? toolFor(ctx.cwd) : bash).execute(id, params, signal, onUpdate),
	});
	pi.on("session_shutdown", async () => {
		if (temporary) rmSync(temporary, { recursive: true, force: true });
	});
}
