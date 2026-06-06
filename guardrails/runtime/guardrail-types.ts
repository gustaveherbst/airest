/**
 * Ambient types for aiREST Deno guardrail scripts.
 * Prepended at transpile time — not executed at runtime.
 */
interface GuardrailEvaluateContext {
  requestId: string;
  endpoint: string;
  version: string;
  hook: string;
  input?: Record<string, unknown>;
  renderedSystem?: string;
  renderedUser?: string;
  llmRaw?: string;
  output?: Record<string, unknown>;
  requestBodyBytes: number;
  config?: Record<string, unknown>;
  auth?: {
    subject?: string;
    tenantId?: string;
    scopes?: string[];
  };
}

type GuardrailAction = "pass" | "block" | "modify" | "warn";

interface GuardrailEvaluateResult {
  action: GuardrailAction;
  message?: string;
  details?: unknown;
  input?: Record<string, unknown>;
  output?: Record<string, unknown>;
}
