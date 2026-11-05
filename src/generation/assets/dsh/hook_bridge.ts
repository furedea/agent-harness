import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";
import type { Context } from "@deepseek-ai/cordis";
import type {} from "@deepseek-ai/dsh-tools";

const ADAPTER = process.env.AGENT_HARNESS_DSH_ADAPTER ??
    join(process.env.DSH_HOME?.trim() || join(homedir(), ".dsh"), "hooks", "hook_adapter.py");

function dispatch(event: string, payload: unknown): Promise<Record<string, unknown>> {
    return new Promise((resolve) => {
        let stdout = "", stderr = "";
        const child = spawn("python3", ["-I", "-B", ADAPTER, "dispatch", event]);
        const timer = setTimeout(() => child.kill("SIGKILL"), 30000);
        child.stdout.on("data", data => { stdout += data; });
        child.stderr.on("data", data => { stderr += data; });
        child.stdin.on("error", () => {});
        child.on("error", error => { clearTimeout(timer); resolve({decision: "block", reason: error.message}); });
        child.on("close", code => {
            clearTimeout(timer);
            let output: Record<string, unknown> = {};
            for (const line of stdout.split("\n")) {
                try { const parsed = JSON.parse(line); if (parsed && typeof parsed === "object") output = parsed; } catch { /* Adapter logs may precede JSON. */ }
            }
            if (code !== 0) output = {decision: "block", reason: stderr.trim() || "agent-harness adapter failed"};
            resolve(output);
        });
        child.stdin.end(JSON.stringify(payload));
    });
}

export function apply(ctx: Context) {
    ctx.on("tools/pre-execute", async (exec, next) => {
        const output = await dispatch("tools/pre-execute", {hook_event_name: "tools/pre-execute", cwd: process.cwd(), tool_name: exec.name, tool_input: exec.arguments});
        if (output.decision === "block" || output.action === "block") {
            return {kind: "deny", reason: String(output.reason ?? output.message ?? "blocked by agent-harness"), info: {name: "AgentHarnessDenied", code: "AGENT_HARNESS_DENIED"}};
        }
        return next();
    });
    ctx.on("tools/post-execute", async (exec, result, next) => {
        const output = await dispatch("tools/post-execute", {hook_event_name: "tools/post-execute", cwd: process.cwd(), tool_name: exec.name, tool_input: exec.arguments, tool_response: result});
        const decision = await next();
        if (typeof output.systemMessage !== "string" || !output.systemMessage) return decision;
        return {...decision, additionalContexts: [...(decision.additionalContexts ?? []), {role: "user", content: [{type: "text", text: output.systemMessage}]}]};
    });
}
