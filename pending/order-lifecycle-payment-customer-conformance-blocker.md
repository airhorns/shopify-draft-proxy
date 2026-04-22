# Order lifecycle, payment, and customer conformance blocker

## What this run checked

HAR-120 added safe validation-probe parity plans for:

- `orderClose`
- `orderOpen`
- `orderMarkAsPaid`
- `orderCreateManualPayment`
- `orderCustomerSet`
- `orderCustomerRemove`
- `orderInvoiceSend`
- `taxSummaryCreate`
- `orderCancel`

Each planned proxy request uses a nonexistent Shopify GID such as `gid://shopify/Order/0` first, so live capture can establish validation/userError semantics before any happy-path mutation can close, reopen, mark paid, create a payment, change a customer association, send email, enqueue tax-summary work, restock/refund, or cancel a merchant order.

## Current run summary

- `corepack pnpm conformance:probe` failed before any root-specific validation probe could run.
- Failure: `Stored Shopify conformance access token is invalid and refresh failed: [API] Invalid API key or access token (unrecognized login or wrong password)`.
- Impact: happy-path fixtures for the HAR-120 roots were not captured in this run.
- Interpretation: this is the same live conformance auth regression class already recorded for current order-domain work; the new parity specs remain `planned` and must not be promoted to captured without fresh Shopify responses.

## Local staging classification

- Candidate for future local staging after fixture-backed semantics: `orderClose`, `orderOpen`, `orderMarkAsPaid`, `orderCreateManualPayment`, `orderCustomerSet`, and `orderCustomerRemove`.
- Keep explicit unsupported passthrough until modeled with side-effect guardrails: `orderInvoiceSend`, `taxSummaryCreate`, and `orderCancel`.
- Keep `orderCancel` especially conservative: happy-path cancellation is irreversible in Shopify and can involve refunds, inventory, fulfillment, and customer notification behavior.

Refresh this note only after the conformance credential can pass `corepack pnpm conformance:probe`.
