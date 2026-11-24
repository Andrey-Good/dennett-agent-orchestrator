-- Dennett Control Schema v8 (Phase D3: Write-Only SecretBroker Vault, Multi-Runtime Provider
-- Connections, A08 Skills/MCP/Plugin Capabilities Registry & Project Bindings, V29 Binding
-- Connection/Endpoint Fingerprints, Local Endpoint Auth Profile).
-- Strictly preserves monotonic sequence counters via `control_stream_counters` (ZERO SERIAL/BIGSERIAL).

-- 1. Write-Only Encrypted SecretBroker Vault (ChaCha20-Poly1305 AEAD, master key stored outside DB)
CREATE TABLE IF NOT EXISTS secret_entries (
    slot_key TEXT PRIMARY KEY CHECK (octet_length(slot_key) > 0 AND octet_length(slot_key) <= 128),
    scope TEXT NOT NULL DEFAULT 'global' CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    description TEXT NOT NULL DEFAULT '' CHECK (octet_length(description) <= 512),
    algorithm TEXT NOT NULL DEFAULT 'chacha20poly1305-v1' CHECK (algorithm = 'chacha20poly1305-v1'),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce) = 12),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) > 0),
    fingerprint_short TEXT NOT NULL CHECK (fingerprint_short ~ '^[0-9a-f]{16}$'),
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 2. Provider Connections Registry (Antigravity ACP dual profiles + Local HTTP runtimes)
CREATE TABLE IF NOT EXISTS provider_connections (
    connection_id UUID PRIMARY KEY,
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('antigravity_acp', 'local_http')),
    display_name TEXT NOT NULL CHECK (octet_length(display_name) > 0 AND octet_length(display_name) <= 128),
    adapter_id TEXT NOT NULL CHECK (octet_length(adapter_id) > 0 AND octet_length(adapter_id) <= 64),
    auth_profile TEXT NOT NULL CHECK (auth_profile IN ('oauth-personal', 'gemini-api-key', 'local-endpoint')),
    endpoint_url TEXT NULL CHECK (endpoint_url IS NULL OR octet_length(endpoint_url) <= 512),
    secret_slot_key TEXT NULL CHECK (
        secret_slot_key IS NULL
        OR (octet_length(secret_slot_key) > 0 AND octet_length(secret_slot_key) <= 128)
    ),
    is_default_interactive BOOLEAN NOT NULL DEFAULT FALSE,
    is_default_background BOOLEAN NOT NULL DEFAULT FALSE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_health_status TEXT NOT NULL DEFAULT 'unknown' CHECK (
        last_health_status IN (
            'healthy',
            'auth_required',
            'unreachable',
            'incompatible',
            'quota_limited',
            'unknown'
        )
    ),
    last_health_detail TEXT NULL CHECK (last_health_detail IS NULL OR octet_length(last_health_detail) <= 1024),
    last_probed_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS provider_connections_default_interactive_uniq
    ON provider_connections(is_default_interactive)
    WHERE is_default_interactive IS TRUE;

CREATE UNIQUE INDEX IF NOT EXISTS provider_connections_default_background_uniq
    ON provider_connections(is_default_background)
    WHERE is_default_background IS TRUE;

-- Idempotent seed of the two separated Google Antigravity (`agy acp`) profiles:
-- - Interactive: Google AI Ultra (`oauth-personal`)
-- - Background: Headless Gemini API key (`gemini-api-key`, bound to `gemini_api_key` slot)
INSERT INTO provider_connections (
    connection_id, provider_kind, display_name, adapter_id, auth_profile,
    endpoint_url, secret_slot_key, is_default_interactive, is_default_background,
    enabled, last_health_status
) VALUES
    (
        '00000000-0000-4000-8000-000000000001',
        'antigravity_acp',
        'Google Antigravity (Google AI Ultra)',
        'antigravity-acp',
        'oauth-personal',
        NULL,
        NULL,
        TRUE,
        FALSE,
        TRUE,
        'unknown'
    ),
    (
        '00000000-0000-4000-8000-000000000002',
        'antigravity_acp',
        'Google Antigravity (Gemini API Key)',
        'antigravity-acp',
        'gemini-api-key',
        NULL,
        'gemini_api_key',
        FALSE,
        TRUE,
        TRUE,
        'unknown'
    )
ON CONFLICT (connection_id) DO NOTHING;

-- 3. Extend runtime_bindings, tasks, and agent_config_versions for D3 multi-runtime routing & V29
ALTER TABLE runtime_bindings DROP CONSTRAINT IF EXISTS runtime_bindings_auth_profile_check;
ALTER TABLE runtime_bindings ADD CONSTRAINT runtime_bindings_auth_profile_check
    CHECK (auth_profile IN ('oauth-personal', 'gemini-api-key', 'local-endpoint'));

ALTER TABLE runtime_bindings
    ADD COLUMN IF NOT EXISTS connection_id UUID NULL REFERENCES provider_connections(connection_id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS endpoint_fingerprint TEXT NULL CHECK (
        endpoint_fingerprint IS NULL OR octet_length(endpoint_fingerprint) <= 256
    );

ALTER TABLE tasks DROP CONSTRAINT IF EXISTS tasks_auth_profile_override_check;
ALTER TABLE tasks ADD CONSTRAINT tasks_auth_profile_override_check
    CHECK (
        auth_profile_override IS NULL
        OR auth_profile_override IN ('oauth-personal', 'gemini-api-key', 'local-endpoint')
    );

ALTER TABLE agent_config_versions
    ADD COLUMN IF NOT EXISTS preferred_connection_id UUID NULL REFERENCES provider_connections(connection_id) ON DELETE SET NULL;

-- 4. A08 Capabilities Registry (Skills, MCP Servers, Plugins) & Per-Project Overrides
CREATE TABLE IF NOT EXISTS capability_records (
    capability_id UUID PRIMARY KEY,
    slug TEXT NOT NULL CHECK (octet_length(slug) > 0 AND octet_length(slug) <= 128),
    kind TEXT NOT NULL CHECK (kind IN ('skill', 'mcp_server', 'plugin')),
    display_name TEXT NOT NULL CHECK (octet_length(display_name) > 0 AND octet_length(display_name) <= 128),
    description TEXT NOT NULL DEFAULT '' CHECK (octet_length(description) <= 4096),
    version TEXT NOT NULL CHECK (octet_length(version) > 0 AND octet_length(version) <= 64),
    origin_kind TEXT NOT NULL CHECK (origin_kind IN ('user_manual', 'project_local', 'personal_discovered', 'builtin')),
    ownership TEXT NOT NULL CHECK (ownership IN ('user', 'project', 'provider', 'dennett')),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global', 'project', 'candidate_unattached')),
    project_id UUID NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    possession_state TEXT NOT NULL CHECK (possession_state IN ('discovered', 'installed', 'connected')),
    trust_state TEXT NOT NULL CHECK (trust_state IN ('unreviewed', 'restricted', 'trusted', 'revoked')),
    activation_state TEXT NOT NULL CHECK (activation_state IN ('disabled', 'on_demand', 'active', 'pinned')),
    compatibility_status TEXT NOT NULL DEFAULT 'compatible' CHECK (compatibility_status IN ('compatible', 'warning', 'incompatible')),
    compatibility_detail TEXT NULL CHECK (compatibility_detail IS NULL OR octet_length(compatibility_detail) <= 1024),
    source_path TEXT NULL CHECK (source_path IS NULL OR octet_length(source_path) <= 1024),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    spec_json JSONB NOT NULL,
    previous_snapshot_json JSONB NULL,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS capability_records_identity_uniq
    ON capability_records(
        slug,
        kind,
        scope_kind,
        COALESCE(project_id, '00000000-0000-4000-8000-000000000000'::uuid),
        origin_kind
    );

CREATE INDEX IF NOT EXISTS capability_records_by_project_kind
    ON capability_records(project_id, kind, activation_state);

CREATE TABLE IF NOT EXISTS capability_project_bindings (
    capability_id UUID NOT NULL REFERENCES capability_records(capability_id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    activation_state TEXT NOT NULL CHECK (activation_state IN ('disabled', 'on_demand', 'active', 'pinned', 'forbidden')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (capability_id, project_id)
);

CREATE INDEX IF NOT EXISTS capability_project_bindings_by_project
    ON capability_project_bindings(project_id, activation_state);
