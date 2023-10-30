#!/usr/bin/env bash
set -euo pipefail

## NOTE: GitHub doesn't seem to want to let us to this very securely.
## NOTE: Hopefully this gets fixed soon.
## NOTE:
## NOTE: For now, repository-level runners have been disabled and are instead
## NOTE: registered at the organization level.
## NOTE:
## NOTE: See my comment for more info and context:
## NOTE: https://github.com/actions/runner/issues/1882#issuecomment-1782061727

function check_env {
	if [ -z "${!1}" ]; then
		echo "missing environment variable: $1" >&2
		exit 2
	fi
}

check_env ACCESS_TOKEN
check_env ORGANIZATION
#check_env REPOSITORY
check_env LABELS
check_env NAME

REG_TOKEN_RAW="$( \
	curl -sX POST \
		-H "Authorization: Bearer ${ACCESS_TOKEN}" \
		-H "Accept: application/vnd.github+json" \
		-H "X-GitHub-Api-Version: 2022-11-28" \
		"https://api.github.com/orgs/${ORGANIZATION}/actions/runners/registration-token")"
#		"https://api.github.com/repos/${ORGANIZATION}/${REPOSITORY}/actions/runners/registration-token")"

REG_TOKEN="$(jq .token --raw-output <<< "${REG_TOKEN_RAW}")"

if [ "$REG_TOKEN" == "null" ]; then
	echo "got 'null' registration token from GitHub:" >&2
	echo "${REG_TOKEN_RAW}" >&2
	exit 1
fi

#	--url "https://github.com/${ORGANIZATION}/${REPOSITORY}" \
./config.sh \
	--url "https://github.com/${ORGANIZATION}" \
	--unattended \
	--token "${REG_TOKEN}" \
	--labels "self-hosted,${LABELS}" \
	--replace \
	--no-default-labels \
	--name "${NAME}"

cleanup() {
	echo "Removing runner..."
	./config.sh remove --unattended --token ${REG_TOKEN}
}

trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

unset ACCESS_TOKEN
unset ORGANIZATION
#unset REPOSITORY
unset LABELS
unset NAME

exec ./run.sh
