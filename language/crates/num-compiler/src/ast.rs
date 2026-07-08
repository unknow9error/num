use crate::span::Span;

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub name: Option<String>,
    pub imports: Vec<Import>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Declaration {
    Permission(PermissionDecl),
    Role(RoleDecl),
    Policy(PolicyDecl),
    Type(TypeDecl),
    Enum(EnumDecl),
    Function(CallableDecl),
    Workflow(CallableDecl),
    Actor(ActorDecl),
    Action(ActionDecl),
    Connector(ConnectorDecl),
    Service(ServiceDecl),
    Test(TestDecl),
    Impl(ImplDecl),
}

impl Declaration {
    pub fn name(&self) -> &str {
        match self {
            Declaration::Permission(decl) => &decl.name,
            Declaration::Role(decl) => &decl.name,
            Declaration::Policy(decl) => &decl.name,
            Declaration::Type(decl) => &decl.name,
            Declaration::Enum(decl) => &decl.name,
            Declaration::Function(decl) => &decl.name,
            Declaration::Workflow(decl) => &decl.name,
            Declaration::Actor(decl) => &decl.name,
            Declaration::Action(decl) => &decl.name,
            Declaration::Connector(decl) => &decl.name,
            Declaration::Service(decl) => &decl.name,
            Declaration::Test(decl) => &decl.name,
            Declaration::Impl(decl) => &decl.target,
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            Declaration::Permission(decl) => &decl.span,
            Declaration::Role(decl) => &decl.span,
            Declaration::Policy(decl) => &decl.span,
            Declaration::Type(decl) => &decl.span,
            Declaration::Enum(decl) => &decl.span,
            Declaration::Function(decl) => &decl.span,
            Declaration::Workflow(decl) => &decl.span,
            Declaration::Actor(decl) => &decl.span,
            Declaration::Action(decl) => &decl.span,
            Declaration::Connector(decl) => &decl.span,
            Declaration::Service(decl) => &decl.span,
            Declaration::Test(decl) => &decl.span,
            Declaration::Impl(decl) => &decl.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PermissionDecl {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RoleDecl {
    pub name: String,
    pub allows: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone)]
pub struct PolicyRule {
    pub effect: PolicyEffect,
    pub privacy: Option<Privacy>,
    pub trust: Option<Trust>,
    pub source: Option<String>,
    pub target: Option<String>,
    pub tenant: Option<String>,
    pub route: Option<PolicyRouteCondition>,
    pub raw: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRouteCondition {
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct PolicyDecl {
    pub name: String,
    pub rules: Vec<PolicyRule>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub generic_params: Vec<String>,
    pub body: TypeBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeBody {
    Struct(Vec<Field>),
    Alias(TypeRef),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: TypeRef,
    pub labels: Labels,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub payload: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CallableDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub result: Option<TypeRef>,
    pub requires: Vec<String>,
    pub budget: Option<String>,
    pub rate_limit: Option<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ActorDecl {
    pub name: String,
    pub state: Vec<ActorStateField>,
    pub handlers: Vec<CallableDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ActorStateField {
    pub name: String,
    pub ty: TypeRef,
    pub labels: Labels,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ActionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub result: Option<TypeRef>,
    pub requires: Vec<String>,
    pub risk: Risk,
    pub rollback: Option<String>,
    pub timeout: Option<String>,
    pub cost: Option<String>,
    pub retry: Option<String>,
    pub idempotency_key: Option<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConnectorDecl {
    pub name: String,
    pub methods: Vec<ConnectorMethod>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConnectorMethod {
    pub name: String,
    pub params: Vec<Param>,
    pub result: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ServiceDecl {
    pub name: String,
    pub budget: Option<String>,
    pub rate_limit: Option<String>,
    pub routes: Vec<ServiceRoute>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ServiceRoute {
    pub method: String,
    pub path: String,
    pub requires: Vec<String>,
    pub input: Option<ServiceInput>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TestDecl {
    pub name: String,
    pub kind: TestKind,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImplDecl {
    pub target: String,
    pub methods: Vec<CallableDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKind {
    Unit,
    Policy,
    Workflow,
    Ai,
}

#[derive(Debug, Clone)]
pub struct ServiceInput {
    pub name: String,
    pub ty: TypeRef,
    pub labels: Labels,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub labels: Labels,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub raw: String,
}

impl TypeRef {
    pub fn is_uncertain(&self) -> bool {
        self.raw.starts_with("Uncertain<")
    }

    pub fn is_option(&self) -> bool {
        self.raw.starts_with("Option<")
    }

    pub fn is_result(&self) -> bool {
        self.raw.starts_with("Result<")
    }

    pub fn is_task(&self) -> bool {
        self.raw.starts_with("Task<")
    }

    pub fn is_secret(&self) -> bool {
        self.raw == "Secret" || self.raw.starts_with("Secret<")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Labels {
    pub source: Option<String>,
    pub trust: Option<Trust>,
    pub privacy: Option<Privacy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trust {
    Untrusted,
    Trusted,
    Verified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Privacy {
    Public,
    Internal,
    Private,
    Sensitive,
    Secret,
    Regulated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for Risk {
    fn default() -> Self {
        Self::Low
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    Assign(AssignStmt),
    Assert(AssertStmt),
    ExpectPolicy(ExpectPolicyStmt),
    ExpectWorkflow(ExpectWorkflowStmt),
    ExpectAudit(ExpectAuditStmt),
    MockAi(MockAiStmt),
    MockAiScan(MockAiScanStmt),
    MockConnector(MockConnectorStmt),
    Require(RequireStmt),
    Transaction(TransactionStmt),
    If(IfStmt),
    Match(MatchStmt),
    Return(RawExpr),
    Expr(RawExpr),
    Scope(ScopeStmt),
}

impl Stmt {
    pub fn span(&self) -> &Span {
        match self {
            Stmt::Let(stmt) => &stmt.span,
            Stmt::Assign(stmt) => &stmt.span,
            Stmt::Assert(stmt) => &stmt.span,
            Stmt::ExpectPolicy(stmt) => &stmt.span,
            Stmt::ExpectWorkflow(stmt) => &stmt.span,
            Stmt::ExpectAudit(stmt) => &stmt.span,
            Stmt::MockAi(stmt) => &stmt.span,
            Stmt::MockAiScan(stmt) => &stmt.span,
            Stmt::MockConnector(stmt) => &stmt.span,
            Stmt::Require(stmt) => &stmt.span,
            Stmt::Transaction(stmt) => &stmt.span,
            Stmt::If(stmt) => &stmt.span,
            Stmt::Match(stmt) => &stmt.span,
            Stmt::Return(expr) | Stmt::Expr(expr) => &expr.span,
            Stmt::Scope(stmt) => &stmt.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScopeStmt {
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LetStmt {
    pub mutable: bool,
    pub name: String,
    pub ty: Option<TypeRef>,
    pub labels: Labels,
    pub expr: Option<RawExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AssignStmt {
    pub name: String,
    pub expr: RawExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AssertStmt {
    pub expr: RawExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExpectPolicyStmt {
    pub outcome: PolicyExpectation,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyExpectation {
    Allow,
    Deny,
}

#[derive(Debug, Clone)]
pub struct ExpectWorkflowStmt {
    pub outcome: WorkflowExpectation,
    pub call: RawExpr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowExpectation {
    Success,
    Failure,
}

#[derive(Debug, Clone)]
pub struct ExpectAuditStmt {
    pub event: RawExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MockAiStmt {
    pub call: RawExpr,
    pub value: RawExpr,
    pub confidence: RawExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MockAiScanStmt {
    pub call: RawExpr,
    pub outcome: String,
    pub reason: Option<RawExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MockConnectorStmt {
    pub call: RawExpr,
    pub value: RawExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RequireStmt {
    pub permission: String,
    pub actor: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TransactionStmt {
    pub saga: bool,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: RawExpr,
    pub then_body: Vec<Stmt>,
    pub else_body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchStmt {
    pub expr: RawExpr,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<RawExpr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    Variant {
        name: String,
        payload: Option<MatchPayloadPattern>,
        bindings: Vec<MatchBinding>,
    },
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPayloadPattern {
    Binding(String),
    Destructure {
        type_name: String,
        bindings: Vec<MatchBinding>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchBinding {
    pub field: String,
    pub name: String,
    pub nested_type: Option<String>,
    pub nested: Vec<MatchBinding>,
}

#[derive(Debug, Clone)]
pub struct RawExpr {
    pub text: String,
    pub span: Span,
}
