# Money exchange-rate example

This example shows the first explicit currency-conversion boundary for
`Money<C>`.

`Money<USD>` and `Money<KZT>` still cannot be added or compared directly. The
workflow must first construct an `ExchangeRate<USD, KZT>` with a decimal rate
and source label, then pass it to `convert_money`.

## Check

```bash
num check examples/money_exchange_rate/src/main.num
```

## Run

```bash
num run examples/money_exchange_rate/src/main.num
```
