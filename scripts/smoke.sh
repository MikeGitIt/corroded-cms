#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:3000}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@corroded.local}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-TemporaryPass123!}"

COOKIE_JAR="$(mktemp "${TMPDIR:-/tmp}/corroded-cms-cookies.XXXXXX")"
BODY_FILE="$(mktemp "${TMPDIR:-/tmp}/corroded-cms-body.XXXXXX")"
HEADER_FILE="$(mktemp "${TMPDIR:-/tmp}/corroded-cms-headers.XXXXXX")"
PNG_FILE="$(mktemp "${TMPDIR:-/tmp}/corroded-cms-upload.XXXXXX")"
BAD_UPLOAD_FILE="$(mktemp "${TMPDIR:-/tmp}/corroded-cms-bad-upload.XXXXXX")"
trap 'rm -f "$COOKIE_JAR" "$BODY_FILE" "$HEADER_FILE" "$PNG_FILE" "$BAD_UPLOAD_FILE"' EXIT

TEST_SLUG="smoke-$(date +%s)"
TEST_TITLE="Smoke Test ${TEST_SLUG}"
DRAFT_SLUG="${TEST_SLUG}-draft"
DRAFT_TITLE="Smoke Draft ${TEST_SLUG}"
TEST_TAG="Smoke Test"
TEST_TAG_SLUG="smoke-test"
EDIT_TAG_NAME="Editable ${TEST_SLUG}"
EDIT_TAG_SLUG="editable-${TEST_SLUG}"
UPDATED_EDIT_TAG_NAME="Edited ${TEST_SLUG}"
UPDATED_EDIT_TAG_SLUG="edited-${TEST_SLUG}"
TEST_ALT_TEXT="Smoke image ${TEST_SLUG}"
UPDATED_ALT_TEXT="Updated smoke image ${TEST_SLUG}"
CSRF_TOKEN=""
COVER_IMAGE_ID=""

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

request() {
    local method="$1"
    local path="$2"
    shift 2

    : >"$HEADER_FILE"
    : >"$BODY_FILE"

    curl -sS \
        -X "$method" \
        -D "$HEADER_FILE" \
        -o "$BODY_FILE" \
        "$@" \
        "${BASE_URL}${path}" >/dev/null
}

status_code() {
    awk 'toupper($0) ~ /^HTTP\// { code=$2 } END { print code }' "$HEADER_FILE"
}

header_value() {
    local name="$1"
    awk -v name="$name" 'BEGIN { IGNORECASE = 1 } $0 ~ "^" name ":" { sub("^[^:]+:[[:space:]]*", ""); sub("\r$", ""); print; exit }' "$HEADER_FILE"
}

assert_status() {
    local expected="$1"
    local actual
    actual="$(status_code)"
    [[ "$actual" == "$expected" ]] || fail "expected HTTP ${expected}, got ${actual}. Body: $(cat "$BODY_FILE")"
}

assert_location() {
    local expected="$1"
    local actual
    actual="$(header_value location)"
    [[ "$actual" == "$expected" ]] || fail "expected Location ${expected}, got ${actual}"
}

assert_header_contains() {
    local name="$1"
    local expected="$2"
    local actual
    actual="$(header_value "$name")"
    [[ "$actual" == *"$expected"* ]] || fail "expected ${name} header to contain ${expected}, got ${actual}"
}

assert_contains() {
    local expected="$1"
    grep -Fq "$expected" "$BODY_FILE" || fail "expected response to contain: ${expected}"
}

assert_not_contains() {
    local unexpected="$1"
    if grep -Fq "$unexpected" "$BODY_FILE"; then
        fail "expected response not to contain: ${unexpected}"
    fi
}

form_post() {
    local path="$1"
    shift
    request POST "$path" -b "$COOKIE_JAR" -c "$COOKIE_JAR" "$@"
}

capture_csrf_token() {
    CSRF_TOKEN="$(sed -n 's/.*name="csrf_token" value="\([^"]*\)".*/\1/p' "$BODY_FILE" | head -n 1)"
    [[ -n "$CSRF_TOKEN" ]] || fail "could not find CSRF token"
}

write_fixture_uploads() {
    local png_b64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
    if ! printf '%s' "$png_b64" | base64 --decode >"$PNG_FILE" 2>/dev/null; then
        printf '%s' "$png_b64" | base64 -D >"$PNG_FILE"
    fi
    printf 'not an image\n' >"$BAD_UPLOAD_FILE"
}

write_fixture_uploads

printf 'Smoke testing %s\n' "$BASE_URL"

request GET /healthz
assert_status 204

request GET /admin
assert_status 303
assert_location /admin/login

request GET /admin/login
assert_status 200
assert_contains "Sign in"

form_post /admin/login \
    --data-urlencode "email=${ADMIN_EMAIL}" \
    --data-urlencode "password=wrong-password"
assert_status 200
assert_contains "Invalid email or password."

for attempt in 1 2 3 4 5; do
    form_post /admin/login \
        --data-urlencode "email=rate-${TEST_SLUG}@corroded.local" \
        --data-urlencode "password=wrong-password"
    assert_status 200
    assert_contains "Invalid email or password."
done

form_post /admin/login \
    --data-urlencode "email=rate-${TEST_SLUG}@corroded.local" \
    --data-urlencode "password=wrong-password"
assert_status 429
assert_contains "Too many login attempts."

form_post /admin/login \
    --data-urlencode "email=${ADMIN_EMAIL}" \
    --data-urlencode "password=${ADMIN_PASSWORD}"
assert_status 303
assert_location /admin

request GET /admin -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "Dashboard"
assert_contains "Published posts"
assert_contains "Recent edits"
capture_csrf_token

request GET /admin/tags -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "Tags"

form_post /admin/tags \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode "name=${EDIT_TAG_NAME}" \
    --data-urlencode "slug=${EDIT_TAG_SLUG}"
assert_status 303
assert_location /admin/tags

request GET /admin/tags -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "$EDIT_TAG_NAME"
assert_contains "$EDIT_TAG_SLUG"
EDIT_TAG_PATH="$(tr '<' '\n' <"$BODY_FILE" | grep -F "$EDIT_TAG_NAME" | sed -n 's/^a href="\([^"]*\)".*/\1/p' | head -n 1)"
[[ "$EDIT_TAG_PATH" == /admin/tags/*/edit ]] || fail "expected editable tag link, got ${EDIT_TAG_PATH}"
EDIT_TAG_ACTION_PATH="${EDIT_TAG_PATH%/edit}"

request GET "$EDIT_TAG_PATH" -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "Edit Tag"
assert_contains "value=\"${EDIT_TAG_NAME}\""

form_post "$EDIT_TAG_ACTION_PATH" \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode "name=${UPDATED_EDIT_TAG_NAME}" \
    --data-urlencode "slug=${UPDATED_EDIT_TAG_SLUG}"
assert_status 303
assert_location /admin/tags

request GET /admin/tags -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "$UPDATED_EDIT_TAG_NAME"
assert_contains "$UPDATED_EDIT_TAG_SLUG"
assert_not_contains "$EDIT_TAG_NAME"

form_post /admin/posts \
    --data-urlencode "title=Missing CSRF" \
    --data-urlencode "slug=${TEST_SLUG}-missing-csrf" \
    --data-urlencode "excerpt=This should fail" \
    --data-urlencode "status=draft" \
    --data-urlencode "tag_slugs=" \
    --data-urlencode "body_markdown=No token"
assert_status 400
assert_contains "invalid CSRF token"

form_post /admin/posts/preview \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode $'body_markdown=# Preview\n\nThis is **rendered**.\n\n<script>alert(1)</script>'
assert_status 200
assert_header_contains content-security-policy "frame-ancestors 'self'"
assert_contains "Markdown Preview"
assert_contains "<strong>rendered</strong>"
assert_not_contains "<script>"

request GET /admin/media -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "Media"

form_post /admin/media \
    -F "csrf_token=${CSRF_TOKEN}" \
    -F "file=@${BAD_UPLOAD_FILE};filename=not-image.txt;type=text/plain"
assert_status 400
assert_contains "image must be a PNG, JPEG, GIF, or WebP file"

form_post /admin/media \
    -F "csrf_token=${CSRF_TOKEN}" \
    -F "file=@${PNG_FILE};filename=smoke.png;type=image/png" \
    -F "alt_text=${TEST_ALT_TEXT}"
assert_status 303
assert_location /admin/media

request GET /admin/media -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "$TEST_ALT_TEXT"
assert_contains "smoke.png"
assert_contains "/uploads/"
assert_contains "1 x 1"
assert_contains "Public URL"
assert_contains "Markdown"
assert_contains "/admin.js"
assert_contains "Copy URL"
assert_contains "Copy Markdown"
UPLOADED_PATH="$(awk 'match($0, /\/uploads\/[^"]+/) { print substr($0, RSTART, RLENGTH); exit }' "$BODY_FILE")"
[[ -n "$UPLOADED_PATH" ]] || fail "could not find uploaded media URL"

request GET "$UPLOADED_PATH"
assert_status 200
assert_header_contains cache-control "max-age=31536000"

request GET /admin/posts/new -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "$UPLOADED_PATH"
assert_contains "Image snippets"
assert_contains "![${TEST_ALT_TEXT}](${UPLOADED_PATH})"
assert_contains "/admin.js"
assert_contains "Advanced"
assert_contains "name=\"scheduled_for\""
assert_contains "name=\"markdown-preview-frame\""
assert_contains "Update Preview"
assert_contains "Insert"
COVER_OPTION="$(tr '<' '\n' <"$BODY_FILE" | grep -F "$UPLOADED_PATH" | head -n 1)"
COVER_IMAGE_ID="$(printf '%s' "$COVER_OPTION" | sed -n 's/.*value="\([0-9a-f-]\{36\}\)".*/\1/p')"
[[ -n "$COVER_IMAGE_ID" ]] || fail "could not find uploaded media option"

form_post "/admin/media/${COVER_IMAGE_ID}" \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode "alt_text=${UPDATED_ALT_TEXT}"
assert_status 303
assert_location /admin/media

request GET /admin/media -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "$UPDATED_ALT_TEXT"
assert_contains "![${UPDATED_ALT_TEXT}](${UPLOADED_PATH})"

form_post /admin/posts/preview \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode "title=Preview ${TEST_SLUG}" \
    --data-urlencode "excerpt=Preview excerpt" \
    --data-urlencode "cover_image_id=${COVER_IMAGE_ID}" \
    --data-urlencode "body_markdown=# Preview

This is **rendered** with a cover image.

![Inline smoke image](${UPLOADED_PATH})"
assert_status 200
assert_contains "Preview ${TEST_SLUG}"
assert_contains "$UPLOADED_PATH"
assert_contains "alt=\"${UPDATED_ALT_TEXT}\""
assert_contains "loading=\"eager\""
assert_contains "fetchpriority=\"high\""
assert_contains "<strong>rendered</strong>"
assert_contains "src=\"${UPLOADED_PATH}\""

request GET /admin/posts -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "Posts"

form_post /admin/posts \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode "title=${TEST_TITLE}" \
    --data-urlencode "slug=${TEST_SLUG}" \
    --data-urlencode "excerpt=Smoke test published post" \
    --data-urlencode "status=published" \
    --data-urlencode "cover_image_id=${COVER_IMAGE_ID}" \
    --data-urlencode "tag_slugs=${TEST_TAG}" \
    --data-urlencode $'body_markdown=# Smoke Test\n\nThis is **scripted** endpoint verification.'
assert_status 303

request GET /admin/posts -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "$TEST_TITLE"

request GET "/admin/posts?status=published&q=${TEST_SLUG}" -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "$TEST_TITLE"

request GET /blog
assert_status 200
assert_header_contains content-security-policy "default-src 'self'"
assert_header_contains x-content-type-options "nosniff"
assert_header_contains x-request-id "-"
assert_header_contains referrer-policy "strict-origin-when-cross-origin"
assert_header_contains permissions-policy "geolocation=()"
assert_contains "$TEST_TITLE"
assert_contains "$UPLOADED_PATH"

request GET /
assert_status 200
assert_contains "$TEST_TITLE"
assert_contains "$UPLOADED_PATH"

request GET "/blog/${TEST_SLUG}"
assert_status 200
assert_contains "$TEST_TITLE"
assert_contains "$UPLOADED_PATH"
assert_contains "alt=\"${UPDATED_ALT_TEXT}\""
assert_contains "property=\"og:image\""
assert_contains "name=\"twitter:card\" content=\"summary_large_image\""
assert_contains "${BASE_URL%/}${UPLOADED_PATH}"
assert_contains "<strong>scripted</strong>"
assert_not_contains "<script>"
assert_contains "loading=\"eager\""
assert_contains "fetchpriority=\"high\""
DETAIL_COVER_PATH="$(tr '<' '\n' <"$BODY_FILE" | sed -n 's/^img class="post-cover" src="\([^"]*\)".*/\1/p' | head -n 1)"
[[ "$DETAIL_COVER_PATH" == "$UPLOADED_PATH" ]] || fail "expected detail cover ${UPLOADED_PATH}, got ${DETAIL_COVER_PATH}"

request GET "$DETAIL_COVER_PATH"
assert_status 200

form_post /admin/posts \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode "title=${DRAFT_TITLE}" \
    --data-urlencode "slug=${DRAFT_SLUG}" \
    --data-urlencode "excerpt=Smoke test draft post" \
    --data-urlencode "status=draft" \
    --data-urlencode "scheduled_for=2035-01-02T03:04" \
    --data-urlencode "tag_slugs=" \
    --data-urlencode "body_markdown=Draft content"
assert_status 303
DRAFT_EDIT_PATH="$(header_value location)"
[[ "$DRAFT_EDIT_PATH" == /admin/posts/*/edit ]] || fail "expected draft edit redirect, got ${DRAFT_EDIT_PATH}"
DRAFT_ACTION_PATH="${DRAFT_EDIT_PATH%/edit}"

request GET "/admin/posts?status=draft&q=${DRAFT_SLUG}" -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "$DRAFT_TITLE"

request GET "/admin/posts?status=published&q=${DRAFT_SLUG}" -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_not_contains "$DRAFT_TITLE"

request GET "/blog/${DRAFT_SLUG}"
assert_status 404

request GET "$DRAFT_EDIT_PATH" -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 200
assert_contains "name=\"workflow_status\" value=\"published\""
assert_contains "value=\"2035-01-02T03:04\""

form_post "$DRAFT_ACTION_PATH" \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode "title=${DRAFT_TITLE}" \
    --data-urlencode "slug=${DRAFT_SLUG}" \
    --data-urlencode "excerpt=Smoke test draft post" \
    --data-urlencode "status=draft" \
    --data-urlencode "workflow_status=published" \
    --data-urlencode "scheduled_for=2035-01-02T03:04" \
    --data-urlencode "cover_image_id=${COVER_IMAGE_ID}" \
    --data-urlencode "tag_slugs=" \
    --data-urlencode "body_markdown=Draft content"
assert_status 303
assert_location "$DRAFT_EDIT_PATH"

request GET "/blog/${DRAFT_SLUG}"
assert_status 200
assert_contains "$DRAFT_TITLE"
assert_contains "$UPLOADED_PATH"

form_post "$DRAFT_ACTION_PATH" \
    --data-urlencode "csrf_token=${CSRF_TOKEN}" \
    --data-urlencode "title=${DRAFT_TITLE}" \
    --data-urlencode "slug=${DRAFT_SLUG}" \
    --data-urlencode "excerpt=Smoke test draft post" \
    --data-urlencode "status=published" \
    --data-urlencode "workflow_status=draft" \
    --data-urlencode "scheduled_for=2035-01-02T03:04" \
    --data-urlencode "cover_image_id=${COVER_IMAGE_ID}" \
    --data-urlencode "tag_slugs=" \
    --data-urlencode "body_markdown=Draft content"
assert_status 303
assert_location "$DRAFT_EDIT_PATH"

request GET "/blog/${DRAFT_SLUG}"
assert_status 404

request GET "/tags/${TEST_TAG_SLUG}"
assert_status 200
assert_contains "$TEST_TITLE"

request GET /feed.xml
assert_status 200
assert_contains "<rss version=\"2.0\">"
assert_contains "$TEST_TITLE"

request GET /rss.xml
assert_status 303
assert_location /feed.xml

request GET /sitemap.xml
assert_status 200
assert_contains "/blog/${TEST_SLUG}"
assert_contains "/tags/${TEST_TAG_SLUG}"

# These negative checks are expected 404s: drafts and missing resources must not publish.
request GET /blog/__missing__
assert_status 404
assert_contains "Page Not Found"

request GET /tags/__missing__
assert_status 404
assert_contains "Page Not Found"

form_post /admin/logout \
    --data-urlencode "csrf_token=${CSRF_TOKEN}"
assert_status 303
assert_location /admin/login

request GET /admin -b "$COOKIE_JAR" -c "$COOKIE_JAR"
assert_status 303
assert_location /admin/login

printf 'Smoke test passed.\n'
