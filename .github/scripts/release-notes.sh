#!/usr/bin/env bash
# Prints the release notes for a tag: its section of CHANGELOG.md, then a link
# to the README's install directions at that tag. Fails when CHANGELOG.md has
# no section for the tag, or an empty one.
#
#   .github/scripts/release-notes.sh <tag> <owner>/<repo>
set -euo pipefail

tag=$1
repo=$2

# The lines between "## <tag>" and the next "## " heading, without the blank
# lines at either end.
section=$(awk -v heading="## $tag" '
  $0 == heading { inside = 1; next }
  inside && /^## / { exit }
  inside && /^[[:space:]]*$/ { blanks = blanks "\n"; next }
  inside { if (printed) printf "%s", blanks; blanks = ""; print; printed = 1 }
' CHANGELOG.md)

if [[ -z $section ]]; then
  echo "CHANGELOG.md has no '## $tag' section, or it is empty" >&2
  exit 1
fi

printf '%s\n\nFor full installation directions, see the [README](https://github.com/%s/blob/%s/README.md#install).\n' \
  "$section" "$repo" "$tag"
