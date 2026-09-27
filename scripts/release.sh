#!/usr/bin/env bash
# Crea y sube el tag de pre-release para un commit, lo que lanza el workflow
# .github/workflows/release.yml.
#
# Formato: pre.<YYYY>.<MM>.<DD>.<sha7>  (fecha del commit en UTC)
#
# Uso:
#   ./scripts/release.sh            # HEAD
#   ./scripts/release.sh <commit>   # commit concreto
#   ./scripts/release.sh --dry-run  # solo muestra el tag

set -euo pipefail

dry_run=false
ref="HEAD"
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry_run=true ;;
        *) ref="$arg" ;;
    esac
done

commit=$(git rev-parse --verify "${ref}^{commit}")
date=$(TZ=UTC0 git show -s --format=%cd --date=format-local:%Y.%m.%d "$commit")
sha=$(git rev-parse --short=7 "$commit")
tag="pre.${date}.${sha}"

echo "Commit: $(git show -s --format='%h %s' "$commit")"
echo "Tag:    ${tag}"

if $dry_run; then
    exit 0
fi

if git rev-parse -q --verify "refs/tags/${tag}" > /dev/null; then
    echo "Error: el tag ${tag} ya existe localmente." >&2
    exit 1
fi

git fetch -q origin
if ! git branch -r --contains "$commit" | grep -q .; then
    echo "Error: el commit ${sha} no está en ningún branch remoto. Haz push primero." >&2
    exit 1
fi

git tag -a "$tag" -m "Pre-release ${tag}" "$commit"
git push origin "refs/tags/${tag}"

echo "Listo. Sigue el progreso en: https://github.com/Debaq/Pinocchiors/actions"
