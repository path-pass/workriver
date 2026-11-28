# Hosted conversation fixtures

`acp-v1-schema.json` is ACP's unmodified stable v1 schema at commit
[`927e318`](https://github.com/agentclientprotocol/agent-client-protocol/blob/927e318/schema/v1/schema.json)
(Schema v1.35.0, [Apache-2.0](LICENSE)). It validates the real journal frames in both directions.

`conversation.json` supplies the session, prompts and expected updates for the shared
conversation. The Codex stand-in under the crate's integration tests generates native
messages for this conversation. The tests read the resulting ACP journal through the
host's Unix socket.
