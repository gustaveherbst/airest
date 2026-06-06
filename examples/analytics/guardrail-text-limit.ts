// Example TypeScript guardrail — loaded via guardrails[].path in endpoint YAML.
function evaluate(ctx: { input: { text?: string } }) {
  const text = ctx.input?.text ?? "";
  if (text.length > 5000) {
    return {
      action: "block",
      message: "Input text exceeds 5000 characters.",
      details: { maxLength: 5000, actualLength: text.length },
    };
  }
  return { action: "pass" };
}
