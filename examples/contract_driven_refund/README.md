# Contract-driven refund example

This example shows how `num` can stay the source of truth while normal backend
code still implements database calls, payment calls, mail delivery, approvals,
and audit storage.

The important boundary is:

```text
src/main.num
  -> generated/refund.contract.json
  -> backend/runtime-demo.js
  -> connector implementations
```

Backend code does not reimplement the workflow. It only provides connector
functions. The runtime reads the contract and enforces permissions, AI
confidence, audit events, and saga rollback.

## Files

- `src/main.num` is the source workflow.
- `generated/refund.contract.json` is the expected machine-readable contract.
- `generated/connectors.d.ts` shows the TypeScript interface a future generator
  should emit for backend connector authors.
- `backend/runtime-demo.js` is a dependency-free Node.js demo runtime.

## Check the `num` source

```bash
cargo run -p num -- check examples/contract_driven_refund/src/main.num
```

## Run backend scenarios

Successful workflow:

```bash
node examples/contract_driven_refund/backend/runtime-demo.js success
```

Low-confidence AI result pauses for human approval:

```bash
node examples/contract_driven_refund/backend/runtime-demo.js approval
```

Missing permission blocks execution:

```bash
node examples/contract_driven_refund/backend/runtime-demo.js denied
```

This exits with code `1` because the contract blocks the workflow before the
refund action.

Mailer failure after refund triggers rollback:

```bash
node examples/contract_driven_refund/backend/runtime-demo.js rollback
```

This also exits with code `1` after the rollback runs, because the original
mailer failure is still returned to the caller.

This is intentionally small. Its purpose is to show the development contract:
`num` owns the workflow, while TypeScript or JavaScript owns only the concrete
connector implementations.
