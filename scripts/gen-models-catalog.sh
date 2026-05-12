#!/usr/bin/env bash
#
# Regenerates data/models.json — the static model catalog embedded into the
# binary (see src/models_catalog.rs). Source: https://models.dev/api.json,
# which lists every provider's model ids.
#
# The committed JSON is the single build-time source of truth, so the build
# stays offline and reproducible. Run this (or let the scheduled CI workflow
# .github/workflows/update-models.yml run it) to refresh the snapshot.
#
# Usage: scripts/gen-models-catalog.sh
# Requires: curl, jq
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$SCRIPT_DIR/../data/models.json"
SRC="https://models.dev/api.json"

# Vendors kept for the curated OpenRouter subset (its full list is ~340 models;
# the rest are reachable on demand via `/models refresh`).
OPENROUTER_VENDORS='["anthropic","openai","google","deepseek","x-ai","meta-llama","mistralai","qwen","moonshotai","z-ai"]'

# models.dev keeps the FULL history of each provider, including retired model ids
# the provider's API now rejects. We can't read "retired" from the data, so we
# drop ids whose last_updated/release_date is older than a per-vendor cutoff.
#
# Cutoffs below were derived by cross-checking models.dev against each provider's
# LIVE /models API on 2026-06-04:
#   anthropic  — retires aggressively; models.dev still lists the 2024 claude-3
#                line (incl. claude-3-7-sonnet, 2025-02) that the API rejects.
#                2025-04-01 drops those and keeps the claude-4.x line + aliases.
#   openai/gemini/openrouter — ZERO stale ids found (these providers keep listed
#                models valid, and openrouter prunes its own), so NO cutoff: a
#                cutoff would only delete still-valid models (e.g. gemini-2.0-flash,
#                gpt-4o). Keys are by nehme-harness provider name; "0000-00-00" = keep all.
# This is a heuristic snapshot — the authoritative list is always the provider's
# live API, reachable via `/models refresh`. Re-tune when a vendor retires a line.
CUTOFFS='{"anthropic":"2025-04-01"}'

echo "Fetching $SRC ... (cutoffs: $CUTOFFS)" >&2
api="$(curl -fsSL --max-time 120 "$SRC")"

# jq program:
#  - `entry`  : project a models.dev model into our compact {id,name,context}.
#  - `denied` : drop non-chat models (embeddings/audio/image/etc.) by id substring,
#               mirroring crate::provider::is_agent_model's denylist, and keep only
#               models that can output text.
#  - emits keys by *nehme-harness* provider name (gemini <- models.dev "google").
echo "$api" | jq --argjson orv "$OPENROUTER_VENDORS" --argjson cut "$CUTOFFS" '
  def deny: [
    "embedding","embed-","text-embedding","gemini-embedding","whisper","transcribe",
    "tts","-audio","realtime","speech","dall-e","gpt-image","image-generation",
    "imagen","sora","veo","moderation","rerank","aqa","davinci-002","babbage-002",
    "stable-diffusion","flux"
  ];
  # ISO date strings compare correctly lexicographically; $c is the vendor cutoff.
  def recent($c): select((.value.last_updated // .value.release_date // "0000") >= $c);
  def chat($c):
    select(.value.modalities.output | index("text"))
    | select((.key | ascii_downcase) as $id | (deny | any(. as $d | $id | contains($d))) | not)
    | recent($c);
  def entry: {id: .value.id, name: .value.name, context: (.value.limit.context // null)};
  def models_of($p; $c): ($p.models // {}) | to_entries | map(chat($c) | entry) | sort_by(.id);
  {
    anthropic:  models_of(.anthropic; ($cut.anthropic  // "0000-00-00")),
    openai:     models_of(.openai;    ($cut.openai     // "0000-00-00")),
    gemini:     models_of(.google;    ($cut.gemini     // "0000-00-00")),
    openrouter: (
      (.openrouter.models // {})
      | to_entries
      | map(chat($cut.openrouter // "0000-00-00")
            | select((.key | split("/")[0]) as $v | $orv | index($v)) | entry)
      | sort_by(.id)
    )
  }
' > "$OUT"

echo "Wrote $OUT" >&2
jq -r 'to_entries[] | "  \(.key): \(.value | length) models"' "$OUT" >&2
