use swc_common::comments::SingleThreadedComments;
use swc_common::errors::Handler;
use swc_common::sync::Lrc;
use swc_common::{Globals, Mark, SourceMap, GLOBALS};
use swc_ecma_ast::EsVersion;
use swc_ecma_codegen::to_code_default;
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};
use swc_ecma_transforms_base::{fixer::fixer, hygiene::hygiene, resolver};
use swc_ecma_transforms_typescript::strip;

use crate::errors::{AiRestError, ErrorType};

const GUARDRAIL_TYPES: &str = include_str!("../../guardrails/runtime/guardrail-types.ts");

pub fn transpile_guardrail_script(user_source: &str) -> Result<String, AiRestError> {
    let source = format!("{GUARDRAIL_TYPES}\n{user_source}");
    transpile_typescript(&source, "guardrail.ts")
}

pub fn transpile_typescript(source: &str, filename: &str) -> Result<String, AiRestError> {
    let cm: Lrc<SourceMap> = Default::default();
    let handler = Handler::with_emitter_writer(Box::new(std::io::sink()), Some(cm.clone()));
    let comments = SingleThreadedComments::default();

    let fm = cm.new_source_file(
        Lrc::new(swc_common::FileName::Custom(filename.to_string())),
        source.to_string(),
    );

    let syntax = Syntax::Typescript(TsSyntax {
        tsx: filename.ends_with(".tsx"),
        ..Default::default()
    });

    let lexer = Lexer::new(
        syntax,
        EsVersion::Es2020,
        StringInput::from(&*fm),
        Some(&comments),
    );
    let mut parser = Parser::new_from(lexer);

    let program = parser
        .parse_program()
        .map_err(|e| e.into_diagnostic(&handler).emit())
        .map_err(|_| transpile_error("Failed to parse TypeScript guardrail script."))?;

    if let Some(err) = parser.take_errors().into_iter().next() {
        err.into_diagnostic(&handler).emit();
        return Err(transpile_error("TypeScript guardrail script has syntax errors."));
    }

    let globals = Globals::default();
    let js = GLOBALS.set(&globals, || {
        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();

        let program = program.apply(resolver(unresolved_mark, top_level_mark, true));
        let program = program.apply(strip(unresolved_mark, top_level_mark));
        let program = program.apply(hygiene());
        let program = program.apply(fixer(Some(&comments)));

        to_code_default(cm, Some(&comments), &program)
    });

    Ok(js)
}

fn transpile_error(message: &str) -> AiRestError {
    AiRestError::new(ErrorType::EndpointDefinition, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transpiles_guardrail_type_annotations() {
        let source = r#"
function evaluate(ctx: GuardrailEvaluateContext): GuardrailEvaluateResult {
  const amount = Number(ctx.input?.amount ?? 0);
  if (amount > 100) {
    return { action: "block", message: "too high" };
  }
  return { action: "pass" };
}
"#;
        let js = transpile_guardrail_script(source).expect("transpile");
        assert!(js.contains("function evaluate"));
        assert!(!js.contains("GuardrailEvaluateContext"));
        assert!(!js.contains("interface GuardrailEvaluateContext"));
        assert!(js.contains("action: \"block\""));
    }
}
