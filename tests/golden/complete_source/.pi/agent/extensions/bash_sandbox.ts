/**
 * Replaces the agent bash tool. Pi and its hooks stay outside srt.
 * Requires srt with recursive denyGlobRegex support (not nixpkgs 0.0.52).
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, realpathSync, writeFileSync, rmSync } from "node:fs";
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
	const match = /^(\*|(?:\*\.)?[a-zA-Z0-9-]+(?:\.[a-zA-Z0-9-]+)*)(?::([0-9]+))?$/.exec(value);
	if (!match) return false;
	const host = match[1];
	if (host === "*") {
		if (!deny) return false;
	} else if (host !== "localhost" && !host.replace(/^\*\./, "").includes(".")) {
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

export default function (pi: ExtensionAPI) {
	const cwd = realpathSync(process.cwd());
	let settingsPath: string | undefined;
	let content: string | undefined;
	let failure: unknown;
	let temporary: string | undefined;
	try {
		const source = fileURLToPath(new URL("./sandbox_policy.json", import.meta.url));
		const policy: unknown = JSON.parse(readFileSync(source, "utf8"));
		validatePolicy(policy);
		for (const key of ["denyRead", "allowWrite", "denyWrite"] as const) {
			policy.filesystem[key] = policy.filesystem[key].map(path => policyPath(path, cwd));
		}
		policy.network.allowUnixSockets = policy.network.allowUnixSockets.map(path => policyPath(path, cwd));
		const common = canonical(execFileSync("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd, encoding: "utf8" }).trim());
		policy.filesystem.allowWrite.push(cwd, common);
		temporary = realpathSync(mkdtempSync(join(tmpdir(), "pi-srt-")));
		settingsPath = join(temporary, "settings.json");
		policy.filesystem.denyWrite.push(
			join(common, "hooks"), join(common, "config"), join(common, "worktrees/*/config.worktree"),
			temporary, canonical(source), fileURLToPath(import.meta.url),
		);
		validatePolicy(policy);
		content = JSON.stringify(policy);
		writeFileSync(settingsPath, content, { mode: 0o600 });
	} catch (error) {
		failure = error;
	}
	const bash = createBashTool(cwd, {
		spawnHook: ({ command, cwd, env }) => {
			if (failure || !settingsPath || !content) throw new Error("Sandbox unavailable: " + String(failure));
			if (readFileSync(settingsPath, "utf8") !== content) throw new Error("Sandbox settings changed");
			return { command: "exec srt --settings " + quote(settingsPath) + " -c " + quote(command), cwd, env };
		},
	});
	pi.registerTool({
		...bash,
		execute: async (id, params, signal, onUpdate, _ctx) => bash.execute(id, params, signal, onUpdate),
	});
	pi.on("session_shutdown", async () => {
		if (temporary) rmSync(temporary, { recursive: true, force: true });
	});
}
