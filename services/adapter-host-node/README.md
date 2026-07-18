# Node Adapter Host

Изолирует Node/TypeScript SDK и provider-specific extensions. Не хранит canonical state и не владеет permissions.

`@modelcontextprotocol/sdk` is a development-only type-resolution workaround
for `@openai/codex-sdk@0.144.5`: the published Codex declaration file imports
its MCP `ContentBlock` type, while the package currently lists that module only
as a development dependency. Runtime code does not import the MCP SDK.

## Codex SDK connectivity canary

Create the dedicated canary login once through the official ChatGPT flow:

```powershell
corepack pnpm --filter @dennett/adapter-host-node run login:codex
```

Then run the live canary:

```powershell
corepack pnpm --filter @dennett/adapter-host-node run canary:codex
```

Both commands refuse API-key and injected access-token environment variables.
The login is stored in a dedicated Codex-managed directory under the user's
local application state; it does not copy or modify the normal Codex login.
The canary pins the built-in OpenAI provider plus ChatGPT login and the exact
SDK/CLI version. It creates an empty temporary Git workspace with isolated Git
configuration and gives Codex a per-run `CODEX_HOME` containing only a hard link
to that dedicated canary credential. The link is on the same volume and inside
the same protected directory as its source. User configuration, plugins,
skills and MCP servers are therefore not loaded, and Dennett never parses or
copies credential values. Temporary workspace, state and links are removed
after the run; any Codex-managed token refresh affects only the dedicated
canary login.

Only normalized versions, event kinds, completion state, continuation identity
and latency are printed. Prompts, responses and credential values are omitted.
