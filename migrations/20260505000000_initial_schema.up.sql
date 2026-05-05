CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE users (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email text NOT NULL UNIQUE,
    password_hash text NOT NULL,
    display_name text NOT NULL CHECK (char_length(display_name) <= 100),
    role text NOT NULL DEFAULT 'admin' CHECK (role IN ('admin')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash text NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE media_assets (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    filename text NOT NULL,
    original_filename text NOT NULL,
    mime_type text NOT NULL,
    size_bytes bigint NOT NULL CHECK (size_bytes >= 0),
    storage_path text NOT NULL UNIQUE,
    width integer CHECK (width IS NULL OR width > 0),
    height integer CHECK (height IS NULL OR height > 0),
    alt_text text CHECK (alt_text IS NULL OR char_length(alt_text) <= 255),
    uploaded_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE posts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    title text NOT NULL CHECK (char_length(title) <= 200),
    slug text NOT NULL UNIQUE CHECK (char_length(slug) <= 200),
    excerpt text NOT NULL DEFAULT '' CHECK (char_length(excerpt) <= 500),
    body_markdown text NOT NULL DEFAULT '' CHECK (octet_length(body_markdown) <= 1048576),
    body_html text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    cover_image_id uuid REFERENCES media_assets(id) ON DELETE SET NULL,
    author_id uuid REFERENCES users(id) ON DELETE SET NULL,
    published_at timestamptz,
    scheduled_for timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE tags (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL CHECK (char_length(name) <= 80),
    slug text NOT NULL UNIQUE CHECK (char_length(slug) <= 100),
    archived_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE post_tags (
    post_id uuid NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    tag_id uuid NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);

CREATE INDEX posts_public_idx ON posts (published_at DESC, created_at DESC)
WHERE status = 'published' AND published_at IS NOT NULL;

CREATE INDEX posts_updated_idx ON posts (updated_at DESC);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);
CREATE INDEX media_assets_uploaded_by_idx ON media_assets (uploaded_by);
