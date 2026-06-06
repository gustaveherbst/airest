/**
 * Healthcare example guardrail: block clinical notes that mention disallowed topics.
 * Contract: evaluate(ctx) -> { action: "pass" | "block" | "modify" | "warn", ... }
 */
function evaluate(ctx: GuardrailEvaluateContext): GuardrailEvaluateResult {
  const note = String(ctx.input?.clinicalNote ?? "");
  const blocked = (ctx.config?.blockedTopics as string[] | undefined) ?? [
    "controlled substance",
    "suicide",
  ];
  const lower = note.toLowerCase();
  for (const topic of blocked) {
    if (lower.includes(String(topic).toLowerCase())) {
      return {
        action: "block",
        message: `Clinical note mentions disallowed topic: ${topic}`,
      };
    }
  }
  return { action: "pass" };
}
