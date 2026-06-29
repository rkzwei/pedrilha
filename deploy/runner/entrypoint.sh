#!/bin/bash
set -euo pipefail

# Fetch a short-lived registration token using the PAT
RESPONSE=$(curl -sX POST \
    -H "Authorization: Bearer ${GITHUB_PAT}" \
    -H "Accept: application/vnd.github.v3+json" \
    "https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/actions/runners/registration-token")

echo "GitHub API response: $RESPONSE"

REG_TOKEN=$(echo "$RESPONSE" | jq -r .token)

if [[ -z "$REG_TOKEN" || "$REG_TOKEN" == "null" ]]; then
    echo "ERROR: failed to get registration token — check GITHUB_PAT and REPO_OWNER/REPO_NAME"
    exit 1
fi

cd /home/runner/actions-runner

./config.sh \
    --url "https://github.com/${REPO_OWNER}/${REPO_NAME}" \
    --token "${REG_TOKEN}" \
    --name "${RUNNER_NAME:-local-runner}" \
    --labels "self-hosted,linux,x64" \
    --work /home/runner/work \
    --unattended \
    --replace

# Deregister cleanly on container stop
cleanup() {
    echo "Deregistering runner..."
    REMOVE_TOKEN=$(curl -sX POST \
        -H "Authorization: Bearer ${GITHUB_PAT}" \
        -H "Accept: application/vnd.github.v3+json" \
        "https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/actions/runners/remove-token" \
        | jq -r .token)
    ./config.sh remove --token "${REMOVE_TOKEN}" || true
}

trap cleanup EXIT INT TERM

./run.sh &
wait $!
