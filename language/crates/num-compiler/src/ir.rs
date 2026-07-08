use crate::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrModule {
    pub name: Option<String>,
    pub items: Vec<IrItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrItem {
    pub kind: IrItemKind,
    pub name: String,
    pub effects: Vec<IrEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrItemKind {
    Permission,
    Role,
    Policy,
    Type,
    Enum,
    Function,
    Workflow,
    Actor,
    Action,
    Connector,
    Service,
    Test,
    Impl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrEffect {
    Permission(String),
    DataPolicy(String),
    TypeAlias(String),
    ConnectorMethod(String),
    ActorState(String),
    ActorHandler(String),
    ServiceRoute(String),
    AuditRequired,
    Rollback(String),
    Cost(String),
    Budget(String),
    RateLimit(String),
    Retry(String),
    IdempotencyKey(String),
    TestKind(String),
    Workflow,
    ExternalAction,
}

pub fn lower(module: &Module) -> IrModule {
    IrModule {
        name: module.name.clone(),
        items: module.declarations.iter().map(lower_decl).collect(),
    }
}

fn lower_decl(decl: &Declaration) -> IrItem {
    match decl {
        Declaration::Permission(permission) => IrItem {
            kind: IrItemKind::Permission,
            name: permission.name.clone(),
            effects: Vec::new(),
        },
        Declaration::Role(role) => IrItem {
            kind: IrItemKind::Role,
            name: role.name.clone(),
            effects: role
                .allows
                .iter()
                .cloned()
                .map(IrEffect::Permission)
                .collect(),
        },
        Declaration::Policy(policy) => IrItem {
            kind: IrItemKind::Policy,
            name: policy.name.clone(),
            effects: policy
                .rules
                .iter()
                .map(|rule| IrEffect::DataPolicy(rule.raw.clone()))
                .collect(),
        },
        Declaration::Type(ty) => IrItem {
            kind: IrItemKind::Type,
            name: ty.name.clone(),
            effects: match &ty.body {
                TypeBody::Struct(_) => Vec::new(),
                TypeBody::Alias(alias) => vec![IrEffect::TypeAlias(alias.raw.clone())],
            },
        },
        Declaration::Enum(en) => IrItem {
            kind: IrItemKind::Enum,
            name: en.name.clone(),
            effects: Vec::new(),
        },
        Declaration::Function(function) => IrItem {
            kind: IrItemKind::Function,
            name: function.name.clone(),
            effects: callable_effects(function, false),
        },
        Declaration::Workflow(workflow) => {
            let effects = callable_effects(workflow, true);
            IrItem {
                kind: IrItemKind::Workflow,
                name: workflow.name.clone(),
                effects,
            }
        }
        Declaration::Actor(actor) => IrItem {
            kind: IrItemKind::Actor,
            name: actor.name.clone(),
            effects: actor
                .state
                .iter()
                .map(|field| IrEffect::ActorState(actor_state_signature(field)))
                .chain(
                    actor
                        .handlers
                        .iter()
                        .map(|handler| IrEffect::ActorHandler(callable_signature(handler))),
                )
                .collect(),
        },
        Declaration::Action(action) => {
            let mut effects = vec![IrEffect::ExternalAction];
            effects.extend(action.requires.iter().cloned().map(IrEffect::Permission));
            if action.risk >= Risk::High {
                effects.push(IrEffect::AuditRequired);
            }
            if let Some(rollback) = &action.rollback {
                effects.push(IrEffect::Rollback(rollback.clone()));
            }
            if let Some(cost) = &action.cost {
                effects.push(IrEffect::Cost(cost.clone()));
            }
            if let Some(retry) = &action.retry {
                effects.push(IrEffect::Retry(retry.clone()));
            }
            if let Some(idempotency_key) = &action.idempotency_key {
                effects.push(IrEffect::IdempotencyKey(idempotency_key.clone()));
            }
            IrItem {
                kind: IrItemKind::Action,
                name: action.name.clone(),
                effects,
            }
        }
        Declaration::Connector(connector) => IrItem {
            kind: IrItemKind::Connector,
            name: connector.name.clone(),
            effects: connector
                .methods
                .iter()
                .map(|method| IrEffect::ConnectorMethod(connector_method_signature(method)))
                .collect(),
        },
        Declaration::Service(service) => IrItem {
            kind: IrItemKind::Service,
            name: service.name.clone(),
            effects: service
                .routes
                .iter()
                .map(|route| IrEffect::ServiceRoute(service_route_signature(route)))
                .chain(service.budget.iter().cloned().map(IrEffect::Budget))
                .chain(service.rate_limit.iter().cloned().map(IrEffect::RateLimit))
                .collect(),
        },
        Declaration::Test(test) => IrItem {
            kind: IrItemKind::Test,
            name: test.name.clone(),
            effects: vec![IrEffect::TestKind(test_kind_name(test.kind).to_string())],
        },
        Declaration::Impl(imp) => IrItem {
            kind: IrItemKind::Impl,
            name: imp.target.clone(),
            effects: Vec::new(),
        },
    }
}

fn actor_state_signature(field: &ActorStateField) -> String {
    format!("{}: {}", field.name, field.ty.raw)
}

fn callable_signature(callable: &CallableDecl) -> String {
    let params = callable
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, param.ty.raw))
        .collect::<Vec<_>>()
        .join(", ");
    let result = callable
        .result
        .as_ref()
        .map(|ty| ty.raw.as_str())
        .unwrap_or("Unit");
    format!("{}({params}) -> {result}", callable.name)
}

fn test_kind_name(kind: TestKind) -> &'static str {
    match kind {
        TestKind::Unit => "unit",
        TestKind::Policy => "policy",
        TestKind::Workflow => "workflow",
        TestKind::Ai => "ai",
    }
}

fn callable_effects(callable: &CallableDecl, workflow: bool) -> Vec<IrEffect> {
    let mut effects = Vec::new();
    if workflow {
        effects.push(IrEffect::Workflow);
    }
    effects.extend(callable.requires.iter().cloned().map(IrEffect::Permission));
    if let Some(budget) = &callable.budget {
        effects.push(IrEffect::Budget(budget.clone()));
    }
    if let Some(rate_limit) = &callable.rate_limit {
        effects.push(IrEffect::RateLimit(rate_limit.clone()));
    }
    effects
}

fn connector_method_signature(method: &ConnectorMethod) -> String {
    let params = method
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, param.ty.raw))
        .collect::<Vec<_>>()
        .join(", ");
    let result = method
        .result
        .as_ref()
        .map(|ty| ty.raw.as_str())
        .unwrap_or("Unit");
    format!("{}({params}) -> {result}", method.name)
}

fn service_route_signature(route: &ServiceRoute) -> String {
    let input = route
        .input
        .as_ref()
        .map(|input| format!(" input {}: {}", input.name, input.ty.raw))
        .unwrap_or_default();
    let requires = if route.requires.is_empty() {
        String::new()
    } else {
        format!(" requires {}", route.requires.join(", "))
    };
    format!("{} \"{}\"{requires}{input}", route.method, route.path)
}
