use crate::guardrails::types::GuardrailOutcome;

#[derive(Debug, Clone)]
pub struct GuardrailModuleRecord {
    pub module: String,
    pub runtime: String,
    pub hook: String,
    pub outcome: String,
}

#[derive(Debug, Default, Clone)]
pub struct GuardrailMetrics {
    pub pass: u64,
    pub block: u64,
    pub modify: u64,
    pub warn: u64,
    pub modules: Vec<GuardrailModuleRecord>,
}

impl GuardrailMetrics {
    pub fn record(
        &mut self,
        module: &str,
        runtime: &str,
        hook: &str,
        outcome: &GuardrailOutcome,
    ) {
        let outcome_str = match outcome {
            GuardrailOutcome::Pass => {
                self.pass += 1;
                "pass"
            }
            GuardrailOutcome::Block { .. } => {
                self.block += 1;
                "block"
            }
            GuardrailOutcome::Modify { .. } => {
                self.modify += 1;
                "modify"
            }
            GuardrailOutcome::Warn { .. } => {
                self.warn += 1;
                "warn"
            }
        };
        self.modules.push(GuardrailModuleRecord {
            module: module.to_string(),
            runtime: runtime.to_string(),
            hook: hook.to_string(),
            outcome: outcome_str.to_string(),
        });
    }
}
