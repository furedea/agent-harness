import { randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import type { Context } from "@deepseek-ai/cordis";
import type { ToolExecution } from "@deepseek-ai/dsh-tools";
import type { MessageId, UserMessage } from "@deepseek-ai/dsh-llm";

declare module "@deepseek-ai/dsh-llm" {
    interface MessageSourceMap {
        "agent-harness": {kind: "agent-harness"};
    }
}

const ADAPTER = process.env.AGENT_HARNESS_DSH_ADAPTER ??
    join(process.env.DSH_HOME?.trim() || join(homedir(), ".dsh"), "hooks", "hook_adapter.py");

function dispatch(event: string, payload: unknown): Promise<Record<string, unknown>> {
    if (!existsSync(ADAPTER)) return Promise.resolve({});
    return new Promise((resolve) => {
        let stdout = "", stderr = "";
        const child = spawn("python3", ["-I", "-B", ADAPTER, "dispatch", event]);
        const timer = setTimeout(() => child.kill("SIGKILL"), 30000);
        child.stdout.on("data", data => { stdout += data; });
        child.stderr.on("data", data => { stderr += data; });
        child.stdin.on("error", () => {});
        child.on("error", error => { clearTimeout(timer); console.error("agent-harness adapter:", error.message); resolve({}); });
        child.on("close", code => {
            clearTimeout(timer);
            let output: Record<string, unknown> = {};
            for (const line of stdout.split("\n")) {
                try { const parsed = JSON.parse(line); if (parsed && typeof parsed === "object") output = parsed; } catch { /* Adapter logs may precede JSON. */ }
            }
            if (code !== 0 && code !== 2) console.error("agent-harness adapter:", stderr.trim() || `exit ${code}`);
            if (code === 2) output = {decision: "block", reason: stderr.trim() || "agent-harness adapter failed"};
            resolve(output);
        });
        child.stdin.end(JSON.stringify(payload));
    });
}

export function apply(ctx: Context) {
    async function workingDirectory(exec: ToolExecution) {
        if (exec.agent && ctx.workingDirectory) {
            try { return await ctx.workingDirectory.ensure(exec.agent, exec.signal) || process.cwd(); }
            catch (error) { console.error("agent-harness working directory:", error); }
        }
        return process.cwd();
    }
    ctx.on("tools/pre-execute", async (exec, next) => {
        const output = await dispatch("tools/pre-execute", {hook_event_name: "tools/pre-execute", cwd: await workingDirectory(exec), tool_name: exec.name, tool_input: exec.arguments});
        if (output.decision === "block" || output.action === "block") {
            return {kind: "deny", reason: String(output.reason ?? output.message ?? "blocked by agent-harness"), info: {name: "AgentHarnessDenied", code: "AGENT_HARNESS_DENIED"}};
        }
        return next();
    });
    ctx.on("tools/post-execute", async (exec, result, next) => {
        const output = await dispatch("tools/post-execute", {hook_event_name: "tools/post-execute", cwd: await workingDirectory(exec), tool_name: exec.name, tool_input: exec.arguments, tool_response: result});
        const decision = await next();
        if (typeof output.systemMessage !== "string" || !output.systemMessage) return decision;
        const message: UserMessage = {
            id: randomUUID() as MessageId,
            source: {kind: "agent-harness"},
            role: "user",
            content: [{type: "text", text: output.systemMessage}],
        };
        return {...decision, additionalContexts: [...(decision.additionalContexts ?? []), message]};
    });
}
