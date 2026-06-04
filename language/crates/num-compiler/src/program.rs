use crate::ast::{Declaration, Module};
use crate::diagnostic::Diagnostic;
use crate::ir::IrModule;
use crate::{ir, lexer, parser, semantic};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct ProgramModule {
    pub source_name: String,
    pub module: Module,
}

#[derive(Debug, Clone)]
pub struct ProgramCheck {
    pub modules: Vec<ProgramModule>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct ProgramCompilation {
    pub modules: Vec<ProgramModule>,
    pub module: Module,
    pub ir: IrModule,
    pub diagnostics: Vec<Diagnostic>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
        }
    }
}

pub fn check(files: Vec<SourceFile>) -> ProgramCheck {
    let (units, mut diagnostics) = parse_files(files);
    let modules_by_name = module_index(&units, &mut diagnostics);
    check_units(&units, &modules_by_name, &mut diagnostics);

    ProgramCheck {
        modules: units,
        diagnostics,
    }
}

pub fn compile(files: Vec<SourceFile>, entry_source_name: Option<&str>) -> ProgramCompilation {
    let (units, mut diagnostics) = parse_files(files);
    let modules_by_name = module_index(&units, &mut diagnostics);
    check_units(&units, &modules_by_name, &mut diagnostics);

    let entry = match entry_source_name {
        Some(source_name) => units.iter().find(|unit| unit.source_name == source_name),
        None => units.first(),
    };

    let module = match entry {
        Some(unit) => {
            let mut ignored_diagnostics = Vec::new();
            let (module, _) = visible_module(unit, &modules_by_name, &mut ignored_diagnostics);
            module
        }
        None => {
            diagnostics.push(
                Diagnostic::error(
                    "N1003",
                    "program entry source was not found",
                    crate::span::Span::synthetic(
                        entry_source_name.unwrap_or("<empty program>").to_string(),
                    ),
                )
                .with_reason("runtime commands need one entry source file from the checked program")
                .with_help(
                    "pass a directory containing src/main.num or a concrete .num entry file",
                ),
            );
            Module::default()
        }
    };
    let ir = ir::lower(&module);

    ProgramCompilation {
        modules: units,
        module,
        ir,
        diagnostics,
    }
}

fn parse_files(files: Vec<SourceFile>) -> (Vec<ProgramModule>, Vec<Diagnostic>) {
    let mut units = Vec::new();
    let mut diagnostics = Vec::new();

    for file in files {
        let lexed = lexer::lex(&file.name, &file.source);
        let parsed = parser::parse(&file.name, &lexed.tokens);
        diagnostics.extend(lexed.diagnostics);
        diagnostics.extend(parsed.diagnostics);
        units.push(ProgramModule {
            source_name: file.name,
            module: parsed.module,
        });
    }

    (units, diagnostics)
}

fn check_units(
    units: &[ProgramModule],
    modules_by_name: &HashMap<String, &ProgramModule>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for unit in units {
        let (visible_module, first_local_declaration) =
            visible_module(unit, &modules_by_name, diagnostics);
        diagnostics.extend(semantic::check_declarations_from(
            &visible_module,
            first_local_declaration,
        ));
        let _ = ir::lower(&visible_module);
    }
}

fn module_index<'a>(
    units: &'a [ProgramModule],
    diagnostics: &mut Vec<Diagnostic>,
) -> HashMap<String, &'a ProgramModule> {
    let mut modules = HashMap::new();
    let mut seen = HashSet::new();

    for unit in units {
        let Some(name) = &unit.module.name else {
            continue;
        };

        if !seen.insert(name.clone()) {
            diagnostics.push(
                Diagnostic::error(
                    "N1001",
                    format!("duplicate module `{name}`"),
                    module_span(unit),
                )
                .with_reason("program checks require each module path to be unique")
                .with_help("rename one module or remove the duplicate file from the check"),
            );
            continue;
        }

        modules.insert(name.clone(), unit);
    }

    modules
}

fn visible_module(
    unit: &ProgramModule,
    modules_by_name: &HashMap<String, &ProgramModule>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Module, usize) {
    let mut declarations = Vec::new();
    let mut visited = HashSet::new();
    let mut resolving = HashSet::new();

    for import in &unit.module.imports {
        collect_imports(
            &import.path,
            modules_by_name,
            diagnostics,
            &mut visited,
            &mut resolving,
            &mut declarations,
            Some(import.span.clone()),
        );
    }

    let first_local_declaration = declarations.len();
    declarations.extend(unit.module.declarations.clone());

    let module = Module {
        name: unit.module.name.clone(),
        imports: unit.module.imports.clone(),
        declarations,
    };

    (module, first_local_declaration)
}

fn collect_imports(
    path: &str,
    modules_by_name: &HashMap<String, &ProgramModule>,
    diagnostics: &mut Vec<Diagnostic>,
    visited: &mut HashSet<String>,
    resolving: &mut HashSet<String>,
    declarations: &mut Vec<Declaration>,
    import_span: Option<crate::span::Span>,
) {
    if !visited.insert(path.to_string()) {
        return;
    }

    let Some(unit) = modules_by_name.get(path) else {
        diagnostics.push(
            Diagnostic::error(
                "N1002",
                format!("unknown module import `{path}`"),
                import_span.unwrap_or_else(|| crate::span::Span::synthetic(path)),
            )
            .with_reason("the imported module was not part of this program check")
            .with_help("add the module file to the checked directory or fix the `use` path"),
        );
        return;
    };

    if !resolving.insert(path.to_string()) {
        return;
    }

    for import in &unit.module.imports {
        collect_imports(
            &import.path,
            modules_by_name,
            diagnostics,
            visited,
            resolving,
            declarations,
            Some(import.span.clone()),
        );
    }

    resolving.remove(path);
    declarations.extend(unit.module.declarations.clone());
}

fn module_span(unit: &ProgramModule) -> crate::span::Span {
    unit.module
        .declarations
        .first()
        .map(|decl| decl.span().clone())
        .unwrap_or_else(|| crate::span::Span::synthetic(unit.source_name.clone()))
}

#[cfg(test)]
mod tests {
    use super::{check, compile, SourceFile};

    fn codes(files: Vec<(&str, &str)>) -> Vec<&'static str> {
        check(
            files
                .into_iter()
                .map(|(name, source)| SourceFile::new(name, source))
                .collect(),
        )
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
    }

    fn sources(files: Vec<(&str, &str)>) -> Vec<SourceFile> {
        files
            .into_iter()
            .map(|(name, source)| SourceFile::new(name, source))
            .collect()
    }

    #[test]
    fn resolves_imported_types_for_program_checks() {
        let diagnostics = codes(vec![
            (
                "src/domain.num",
                r#"
module app.domain

type RefundRequest {
    reason: Text
}
"#,
            ),
            (
                "src/main.num",
                r#"
module app.main
use app.domain

workflow main(request: RefundRequest) {
    audit(request.reason)
}
"#,
            ),
        ]);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn reports_unknown_imports() {
        let diagnostics = codes(vec![(
            "src/main.num",
            r#"
module app.main
use app.missing

workflow main() {
    audit("x")
}
"#,
        )]);

        assert!(diagnostics.contains(&"N1002"));
    }

    #[test]
    fn reports_duplicate_modules() {
        let diagnostics = codes(vec![
            (
                "src/a.num",
                r#"
module app.same

workflow a() {
    audit("a")
}
"#,
            ),
            (
                "src/b.num",
                r#"
module app.same

workflow b() {
    audit("b")
}
"#,
            ),
        ]);

        assert!(diagnostics.contains(&"N1001"));
    }

    #[test]
    fn compiles_entry_with_imported_declarations_visible_to_runtime() {
        let program = compile(
            sources(vec![
                (
                    "src/domain.num",
                    r#"
module app.domain

type RefundRequest {
    reason: Text
}

fn record_reason(request: RefundRequest) {
    audit(request.reason)
}
"#,
                ),
                (
                    "src/main.num",
                    r#"
module app.main
use app.domain

workflow main(request: RefundRequest) {
    record_reason(request)
}
"#,
                ),
            ]),
            Some("src/main.num"),
        );

        assert!(program.diagnostics.is_empty());
        assert!(program
            .module
            .declarations
            .iter()
            .any(|declaration| declaration.name() == "RefundRequest"));
        assert!(program
            .module
            .declarations
            .iter()
            .any(|declaration| declaration.name() == "main"));
        assert_eq!(program.ir.name.as_deref(), Some("app.main"));
    }
}
