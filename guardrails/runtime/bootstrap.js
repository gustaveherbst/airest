// Embedded guardrail runtime bootstrap (executed before every custom module).
"use strict";

globalThis.AirestGuardrail = {
  log(level, message) {
    if (typeof Deno !== "undefined" && Deno.core?.ops?.op_airest_guardrail_log) {
      Deno.core.ops.op_airest_guardrail_log(level, message);
    }
  },

  normalizeOutcome(raw) {
    if (!raw || typeof raw !== "object") {
      throw new Error("Guardrail evaluate() must return an object");
    }
    const action = raw.action;
    if (typeof action !== "string") {
      throw new Error("Guardrail outcome requires action: pass|block|modify|warn");
    }
    switch (action) {
      case "pass":
        return { action: "pass" };
      case "warn":
        if (typeof raw.message !== "string" || !raw.message) {
          throw new Error("warn outcome requires message");
        }
        return { action: "warn", message: raw.message };
      case "block":
        if (typeof raw.message !== "string" || !raw.message) {
          throw new Error("block outcome requires message");
        }
        return {
          action: "block",
          message: raw.message,
          details: raw.details ?? null,
        };
      case "modify":
        return {
          action: "modify",
          input: raw.input ?? null,
          output: raw.output ?? null,
        };
      default:
        throw new Error(`Unknown guardrail action: ${action}`);
    }
  },
};
