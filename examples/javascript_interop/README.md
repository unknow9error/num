# JavaScript interop

This example keeps Num as the workflow and policy boundary while delegating one
small implementation step to a local JavaScript module.

`src/main.num` declares a typed connector method:

```num
connector profile {
    enrich(id: Text from UserInput private, email: Text from UserInput private) -> EnrichedProfile
}
```

`num.toml` binds that method under `[javascript]`:

```toml
[javascript]
"profile.enrich" = { module = "interop/profile.cjs", export = "enrich", timeout_ms = "1500" }
```

The JavaScript export receives a single envelope:

```js
exports.enrich = async ({ args, context }) => ({ /* JSON-compatible value */ });
```

Use this boundary for small local JS/TS integration points where JSON values and
runtime context are enough. Prefer `connector` plus `num connector-sdk` for
typed production integrations that need a stable implementation contract.
