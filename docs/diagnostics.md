# num Diagnostics

Diagnostics are emitted by the lexer, parser, and semantic checker. Each
diagnostic has:

- a code such as `N2100`;
- severity: `error`, `warning`, or `info`;
- source location;
- message;
- optional reason;
- optional help text.

## Lexer

- `N0001` - unexpected character.
- `N0002` - unterminated string literal.

## Parser

- `N0100` - expected a top-level declaration.
- `N0101` - expected an identifier or name in the current syntactic position.
- `N0102` - expected a required symbol, for example `{`, `}`, `(`, `)`, or `:`.
- `N0103` - a top-level declaration appears inside an unclosed block.

## Declarations and Names

- `N1000` - duplicate top-level declaration name.
- `N1001` - duplicate module path in a program check.
- `N1002` - `use` imports a module path that is not part of the checked
  program.
- `N1003` - runtime program compilation could not find the selected entry
  source.
- `N1100` - role allows an unknown permission.
- `N1101` - permission reference points to an undeclared permission.
- `N1200` - duplicate field inside a `type`.
- `N1201` - unknown type reference.
- `N1202` - duplicate generic parameter inside a `type`.
- `N1203` - generic type reference has the wrong number of arguments.
- `N1300` - expression result type is incompatible with an explicit binding
  type.
- `N1301` - field access references a field not present on the value type.
- `N1302` - `return` value is missing or incompatible with the declared
  callable result type.
- `N1303` - a callable without `-> Type` returns a value.
- `N1304` - assignment targets an immutable binding.
- `N1305` - assignment targets an unknown binding.
- `N1306` - not all control-flow paths return a value from a typed callable.
- `N1400` - `match` is used with a non-enum/non-union expression.
- `N1401` - `match` arm references an unknown enum variant or union member.
- `N1402` - `match` contains duplicate variant or wildcard arms.
- `N1403` - `match` is non-exhaustive and has no wildcard arm.
- `N1404` - `match` destructuring references a non-struct member, unsupported
  payload shape, or unknown field.
- `N1405` - `match` destructuring repeats a field or binding name.
- `N1406` - `match` guard expression is not `Bool`.

## Actions, Audit, and Rollback

- `N2001` - high-risk action does not write an audit event.
- `N2002` - high-risk action has no rollback metadata.
- `N2500` - action call is missing a required `require Permission.<Name>`.
- `N2600` - saga calls a high-risk action without rollback metadata.

## AI and Uncertainty

- `N2100` - AI call result is assigned to a non-`Uncertain<T>` type.
- `N2300` - uncertain value is used without confidence handling.
- `N2301` - `Option<T>.value` is used without an `is_some` guard or a prior
  terminal `is_none` guard.
- `N2302` - `Result<T,E>.value` or `.error` is used without an `is_ok` or
  `is_err` guard, or the corresponding prior terminal inverse guard.
- `N2303` - `?` is used on an expression that is not `Result<T,E>`.
- `N2304` - `?` is used where the enclosing callable does not return a
  compatible `Result<_,E>`.
- `N2305` - `Ok(...)` or `Err(...)` is used outside an expected `Result<T,E>`
  context or with invalid constructor arity.
- `N2306` - `Ok(...)` or `Err(...)` payload type does not match `Result<T,E>`.
- `N2307` - an `Option<T>` constructor has invalid arity or cannot infer/receive
  an expected `Option<T>` context.
- `N2310` - brand constructor is called with the wrong number of arguments.
- `N2311` - brand constructor payload is incompatible with the brand base type.
- `N2312` - generic brand constructor cannot infer or receive concrete branded
  alias type arguments.
- `N2313` - `unbrand(...)` is called with the wrong number of arguments.
- `N2314` - `unbrand(...)` is called with a non-branded value.
- `N2308` - `Some(...)` payload type does not match `Option<T>`.
- `N2320` - enum variant constructor cannot infer a unique enum context or does
  not belong to the expected enum type.
- `N2321` - enum variant constructor arity does not match the declared payload.
- `N2322` - enum variant constructor payload type does not match the declared
  payload type.

## Async Tasks

- `N2900` - `await` is used on an expression that is not `Task<T>`.
- `N2901` - an `async` task is created as a bare expression statement without
  an owner.

The current checker treats calls whose expression text contains `ai.` as AI
calls. It treats use of `.confidence`, `.value`, `require_human_review`, or
`require_human_approval` as uncertainty handling.

## Privacy and Secrets

- `N2200` - secret value is logged through `log`.
- `N2400` - private, sensitive, secret, or regulated data flows to
  `ExternalApi`, `ConnectorApi`, or a specific external/connector target without
  an allow policy.
- `N2410` - untrusted data is assigned to a `trusted` or `verified` binding
  without an explicit trust gateway.
- `N2411` - untrusted data flows into an external service or high-risk action.
- `N2412` - untrusted data flows into an `ai.*` prompt or tool call without an
  explicit trust gateway.
- `N3100` - `assert` expression does not type-check as `Bool`.
- `N3101` - `expect_deny` did not observe a policy-denial diagnostic in its
  nested block.
- `N3102` - `expect_allow` observed a policy-denial diagnostic in its nested
  block.
- `N3103` - `expect_deny` or `expect_allow` was used outside a `test policy`
  block.
- `N3104` - `expect_workflow_success` or `expect_workflow_failure` was used
  outside a `test workflow` block.
- `N3105` - a workflow expectation did not contain a direct declared workflow
  call.
- `N3106` - `mock_ai` was used outside a `test ai` block.
- `N3107` - `mock_ai` did not target a direct `ai.*` connector call.
- `N3108` - `mock_ai` targets an undeclared AI connector method.
- `N3109` - `mock_ai` targets an AI connector method that does not return
  `Uncertain<T>`.
- `N3110` - `mock_ai` value is incompatible with the inner type of
  `Uncertain<T>`.
- `N3111` - `mock_ai` confidence is not numeric.
- `N3112` - `mock_connector` was used outside a `test workflow` block.
- `N3113` - `mock_connector` did not target a direct non-AI connector method
  call.
- `N3114` - `mock_connector` targets an undeclared connector method.
- `N3115` - `mock_connector` targets a connector method without a non-`Unit`
  result.
- `N3116` - `mock_connector` value is incompatible with the connector method
  result type.
- `N3117` - `expect_audit` was used outside a `test workflow` block.
- `N3118` - `mock_ai_scan` was used outside a `test ai` block.
- `N3119` - `mock_ai_scan` did not target a direct `ai.*` connector call.
- `N3120` - `mock_ai_scan` uses an unknown scanner outcome.
- `N3121` - `mock_ai_scan` reason is not `Text`.

The current data-flow check recognizes `external.*` calls, declared connector
method calls, `ExternalApi`/`ConnectorApi` targets, high-risk action calls, and
`ai.*` prompt/tool-call sinks. It uses source/privacy/trust labels from
parameters, type fields, nested structured fields, and local `let` bindings.
`anonymize(...)` is the explicit privacy gateway: it declassifies the result to
public `DerivedData`; trust gateways such as `sanitize(...)` do not remove
privacy or provenance labels.

At runtime, connector calls preserve the checked boundary as an egress context
that includes connector/method identity, scoped capability, actor, tenant,
request/correlation identifiers, a policy decision marker, and declared
argument source/privacy/trust labels. Manifest-configured process connectors
receive this context in stdin under `egress`, so distributed connector workers
can enforce and audit the same data-leak contract outside the originating Num
runtime instance.

## External Calls

- `N2700` - member call uses an undeclared connector or service namespace.
- `N2701` - direct function/action call does not resolve to a declared callable
  or built-in runtime function.
- `N2702` - connector method is duplicated or a call references an undeclared
  method.
- `N2703` - connector method call has the wrong number of arguments.
- `N2704` - connector method call argument type does not match the connector
  schema.
- `N2705` - direct `fn`, `workflow`, `action`, built-in validator, hash helper,
  date/time helper, or decimal helper call has the wrong number of arguments.
- `N2706` - direct `fn`, `workflow`, `action`, built-in validator, hash helper,
  date/time helper, or decimal helper call argument type does not match the
  callable signature.
- `N2707` - built-in scalar validator, date/time helper, or decimal helper was
  called with a literal that cannot satisfy the requested scalar/date-time type.
- `N2800` - duplicate service route method/path inside a `service`.

## Expressions

- `N3000` - expression syntax is not supported by the current expression parser.
- `N3001` - ordering comparison uses incompatible or non-ordered scalar types.
- `N3002` - equality comparison uses incompatible operand types.
- `N3003` - boolean operator uses non-`Bool` operands.
- `N3004` - arithmetic operator uses incompatible numeric or `Money<C>`
  operand types.

## Lints

- `N4000` - source has declarations but no explicit `module` path.
- `N4001` - high-risk action has no timeout metadata.
- `N4002` - high-risk action has no cost metadata.
- `N4003` - high-risk action has no idempotency key.
- `N4004` - service route has no permission requirement.
- `N4005` - private, sensitive, or regulated value has no provenance source.
- `N4006` - `Secret<T>` value is missing the explicit `secret` privacy label.
  The type is still treated as secret by semantic flow checks; this lint keeps
  source declarations explicit.

## Error Handling Contract

`num check` exits unsuccessfully when any diagnostic has `error` severity.
Warnings are printed but do not fail the command.

`num lint` exits unsuccessfully when it emits any parser, semantic, or lint
diagnostic.
