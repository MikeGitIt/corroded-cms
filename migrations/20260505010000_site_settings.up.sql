CREATE TABLE site_settings (
    key text PRIMARY KEY CHECK (char_length(key) <= 100),
    value text NOT NULL CHECK (char_length(value) <= 5000),
    updated_at timestamptz NOT NULL DEFAULT now()
);
