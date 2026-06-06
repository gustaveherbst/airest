/**
 * Support example guardrail: enterprise tickets cannot be downgraded below high priority in input.
 */
function evaluate(ctx: GuardrailEvaluateContext): GuardrailEvaluateResult {
  const tier = String(ctx.input?.customerTier ?? "");
  const subject = String(ctx.input?.subject ?? "");
  if (tier === "enterprise" && subject.toLowerCase().includes("outage")) {
    return {
      action: "warn",
      message: "Enterprise outage ticket — ensure on-call routing",
    };
  }
  return { action: "pass" };
}
