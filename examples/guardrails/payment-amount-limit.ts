/**
 * Finance example guardrail: block payment requests above a configured limit.
 */
function evaluate(ctx: GuardrailEvaluateContext): GuardrailEvaluateResult {
  const amount = Number(ctx.input?.amount ?? 0);
  const currency = String(ctx.input?.currency ?? "USD");
  const max = Number(ctx.config?.maxAmount ?? 10000);
  if (!Number.isFinite(amount) || amount <= 0) {
    return { action: "block", message: "Payment amount must be a positive number." };
  }
  if (amount > max) {
    return {
      action: "block",
      message: `Payment of ${amount} ${currency} exceeds limit of ${max}`,
    };
  }
  return { action: "pass" };
}
